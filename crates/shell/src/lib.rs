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
            _ => todo!(),
        }
    }

    /// Return a list of all possible interactive profile paths.
    pub fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = vec![];

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
            _ => {}
        };

        #[cfg(unix)]
        {
            profiles.push(home_dir.join(".profile"));
        }

        profiles
    }
}
