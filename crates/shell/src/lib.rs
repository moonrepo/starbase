use dirs::config_dir;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Ion,
    Nushell,
    Powershell,
    Zsh,
}

impl Shell {
    /// Return the profile path that should be used for interactive shells.
    /// This is also the profile that environment variables will be written to.
    pub fn get_main_profile_path(&self, home_dir: &Path) -> PathBuf {
        match self {
            Self::Bash => home_dir.join(".bash_profile"),
            Self::Elvish => get_config_dir(home_dir).join("elvish").join("rc.elv"),
            Self::Fish => get_config_dir(home_dir).join("fish").join("config.fish"),
            _ => todo!(),
        }
    }

    /// Return a list of all possible interactive profile paths.
    pub fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = HashSet::new();

        match self {
            // https://www.baeldung.com/linux/bashrc-vs-bash-profile-vs-profile
            Self::Bash => {
                profiles.extend([
                    home_dir.join(".bash_profile"),
                    home_dir.join(".bash_login"),
                    home_dir.join(".bashrc"),
                    home_dir.join(".profile"),
                ]);
            }
            // https://elv.sh/ref/command.html#using-elvish-interactively
            Self::Elvish => {
                profiles.extend([
                    get_config_dir(home_dir).join("elvish").join("rc.elv"),
                    home_dir.join(".config").join("elvish").join("rc.elv"),
                    home_dir.join(".elvish").join("rc.elv"), // Legacy
                ]);

                #[cfg(windows)]
                {
                    profiles.insert(
                        home_dir
                            .join("AppData")
                            .join("Roaming")
                            .join("elvish")
                            .join("rc.elv"),
                    );
                }
            }
            // https://fishshell.com/docs/current/language.html#configuration
            Self::Fish => {
                // No others
            }
            _ => {}
        };

        #[cfg(unix)]
        {
            profiles.insert(home_dir.join(".profile"));
        }

        profiles.into_iter().collect()
    }
}

fn get_config_dir(home_dir: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(|xdg| {
            let dir = PathBuf::from(&xdg);

            if !xdg.is_empty() && dir.is_absolute() {
                Some(dir)
            } else {
                None
            }
        })
        .unwrap_or_else(|| home_dir.join(".config"))
}
