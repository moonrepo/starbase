// This code is copied from indicatif: https://github.com/console-rs/indicatif/blob/main/src/state.rs#L410
// All code is copyright console-rs: https://github.com/console-rs/indicatif/blob/main/LICENSE

use std::time::{Duration, Instant};

/// Double-smoothed exponentially weighted estimator
///
/// This uses an exponentially weighted *time-based* estimator, meaning that it exponentially
/// downweights old data based on its age. The rate at which this occurs is currently a constant
/// value of 15 seconds for 90% weighting. This means that all data older than 15 seconds has a
/// collective weight of 0.1 in the estimate, and all data older than 30 seconds has a collective
/// weight of 0.01, and so on.
///
/// The primary value exposed by `Estimator` is `steps_per_second`. This value is doubly-smoothed,
/// meaning that is the result of using an exponentially weighted estimator (as described above) to
/// estimate the value of another exponentially weighted estimator, which estimates the value of
/// the raw data.
///
/// The purpose of this extra smoothing step is to reduce instantaneous fluctations in the estimate
/// when large updates are received. Without this, estimates might have a large spike followed by a
/// slow asymptotic approach to zero (until the next spike).
#[derive(Debug)]
pub struct Estimator {
    smoothed_steps_per_sec: f64,
    double_smoothed_steps_per_sec: f64,
    prev_steps: u64,
    prev_time: Instant,
    start_time: Instant,
}

impl Estimator {
    pub fn new() -> Self {
        let now = Instant::now();

        Self {
            smoothed_steps_per_sec: 0.0,
            double_smoothed_steps_per_sec: 0.0,
            prev_steps: 0,
            prev_time: now,
            start_time: now,
        }
    }

    pub fn record(&mut self, new_steps: u64, now: Instant) {
        // sanity check: don't record data if time or steps have not advanced
        if new_steps <= self.prev_steps || now <= self.prev_time {
            // Reset on backwards seek to prevent breakage from seeking to the end for length determination
            // See https://github.com/console-rs/indicatif/issues/480
            if new_steps < self.prev_steps {
                self.prev_steps = new_steps;
                self.reset(now);
            }
            return;
        }

        let delta_steps = new_steps - self.prev_steps;
        let delta_t = duration_to_secs(now - self.prev_time);

        // the rate of steps we saw in this update
        let new_steps_per_second = delta_steps as f64 / delta_t;

        // update the estimate: a weighted average of the old estimate and new data
        let weight = estimator_weight(delta_t);
        self.smoothed_steps_per_sec =
            self.smoothed_steps_per_sec * weight + new_steps_per_second * (1.0 - weight);

        // An iterative estimate like `smoothed_steps_per_sec` is supposed to be an exponentially
        // weighted average from t=0 back to t=-inf; Since we initialize it to 0, we neglect the
        // (non-existent) samples in the weighted average prior to the first one, so the resulting
        // average must be normalized. We normalize the single estimate here in order to use it as
        // a source for the double smoothed estimate. See comment on normalization in
        // `steps_per_second` for details.
        let delta_t_start = duration_to_secs(now - self.start_time);
        let total_weight = 1.0 - estimator_weight(delta_t_start);
        let normalized_smoothed_steps_per_sec = self.smoothed_steps_per_sec / total_weight;

        // determine the double smoothed value (EWA smoothing of the single EWA)
        self.double_smoothed_steps_per_sec = self.double_smoothed_steps_per_sec * weight
            + normalized_smoothed_steps_per_sec * (1.0 - weight);

        self.prev_steps = new_steps;
        self.prev_time = now;
    }

    /// Reset the state of the estimator. Once reset, estimates will not depend on any data prior
    /// to `now`. This does not reset the stored position of the progress bar.
    pub fn reset(&mut self, now: Instant) {
        self.smoothed_steps_per_sec = 0.0;
        self.double_smoothed_steps_per_sec = 0.0;

        // only reset prev_time, not prev_steps
        self.prev_time = now;
        self.start_time = now;
    }

    /// Average time per step in seconds, using double exponential smoothing
    pub fn steps_per_second(&self, now: Instant) -> f64 {
        // Because the value stored in the Estimator is only updated when the Estimator receives an
        // update, this value will become stuck if progress stalls. To return an accurate estimate,
        // we determine how much time has passed since the last update, and treat this as a
        // pseudo-update with 0 steps.
        let delta_t = duration_to_secs(now - self.prev_time);
        let reweight = estimator_weight(delta_t);

        // Normalization of estimates:
        //
        // The raw estimate is a single value (smoothed_steps_per_second) that is iteratively
        // updated. At each update, the previous value of the estimate is downweighted according to
        // its age, receiving the iterative weight W(t) = 0.1 ^ (t/15).
        //
        // Since W(Sum(t_n)) = Prod(W(t_n)), the total weight of a sample after a series of
        // iterative steps is simply W(t_e) - W(t_b), where t_e is the time since the end of the
        // sample, and t_b is the time since the beginning. The resulting estimate is therefore a
        // weighted average with sample weights W(t_e) - W(t_b).
        //
        // Notice that the weighting function generates sample weights that sum to 1 only when the
        // sample times span from t=0 to t=inf; but this is not the case. We have a first sample
        // with finite, positive t_b = t_f. In the raw estimate, we handle times prior to t_f by
        // setting an initial value of 0, meaning that these (non-existent) samples have no weight.
        //
        // Therefore, the raw estimate must be normalized by dividing it by the sum of the weights
        // in the weighted average. This sum is just W(0) - W(t_f), where t_f is the time since the
        // first sample, and W(0) = 1.
        let delta_t_start = duration_to_secs(now - self.start_time);
        let total_weight = 1.0 - estimator_weight(delta_t_start);

        // Generate updated values for `smoothed_steps_per_sec` and `double_smoothed_steps_per_sec`
        // (sps and dsps) without storing them. Note that we normalize sps when using it as a
        // source to update dsps, and then normalize dsps itself before returning it.
        let sps = self.smoothed_steps_per_sec * reweight / total_weight;
        let dsps = self.double_smoothed_steps_per_sec * reweight + sps * (1.0 - reweight);

        dsps / total_weight
    }

    pub fn calculate_eta(&self, value: u64, max: u64) -> Duration {
        let steps_per_second = self.steps_per_second(Instant::now());

        if steps_per_second == 0.0 {
            return Duration::new(0, 0);
        }

        secs_to_duration(max.saturating_sub(value) as f64 / steps_per_second)
    }

    pub fn calculate_sps(&self) -> f64 {
        self.steps_per_second(Instant::now())
    }
}

fn duration_to_secs(d: Duration) -> f64 {
    d.as_secs() as f64 + f64::from(d.subsec_nanos()) / 1_000_000_000f64
}

fn secs_to_duration(s: f64) -> Duration {
    let secs = s.trunc() as u64;
    let nanos = (s.fract() * 1_000_000_000f64) as u32;
    Duration::new(secs, nanos)
}

const EXPONENTIAL_WEIGHTING_SECONDS: f64 = 15.0;

fn estimator_weight(age: f64) -> f64 {
    0.1_f64.powf(age / EXPONENTIAL_WEIGHTING_SECONDS)
}
