use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
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
            Self::Ion => get_config_dir(home_dir).join("ion").join("initrc"),
            Self::Nushell => get_config_dir(home_dir).join("nushell").join("config.nu"),
            Self::Powershell => {
                #[cfg(windows)]
                {
                    home_dir
                        .join("Documents")
                        .join("PowerShell")
                        .join("Microsoft.PowerShell_profile.ps1")
                }

                #[cfg(not(windows))]
                {
                    get_config_dir(home_dir)
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1")
                }
            }
            Self::Zsh => env::var_os("ZDOTDIR")
                .and_then(is_absolute_dir)
                .unwrap_or(home_dir.to_owned())
                .join(".zprofile"),
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
                profiles.extend([
                    get_config_dir(home_dir).join("fish").join("config.fish"),
                    home_dir.join(".config").join("fish").join("config.fish"),
                ]);
            }
            // https://doc.redox-os.org/ion-manual/general.html#xdg-app-dirs-support
            Self::Ion => {
                profiles.extend([
                    get_config_dir(home_dir).join("ion").join("initrc"),
                    home_dir.join(".config").join("ion").join("initrc"),
                ]);
            }
            // https://www.nushell.sh/book/configuration.html
            Self::Nushell => {
                profiles.extend([
                    get_config_dir(home_dir).join("nushell").join("config.nu"),
                    home_dir.join(".config").join("nushell").join("config.nu"),
                ]);
            }
            // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-7.4
            Self::Powershell => {
                #[cfg(windows)]
                {
                    let docs_dir = user_dir.join("Documents");

                    profiles.extend([
                        docs_dir
                            .join("PowerShell")
                            .join("Microsoft.PowerShell_profile.ps1"),
                        docs_dir.join("PowerShell").join("Profile.ps1"),
                    ]);
                }

                #[cfg(not(windows))]
                {
                    profiles.extend([
                        get_config_dir(home_dir)
                            .join("powershell")
                            .join("Microsoft.PowerShell_profile.ps1"),
                        home_dir
                            .join(".config")
                            .join("powershell")
                            .join("Microsoft.PowerShell_profile.ps1"),
                        get_config_dir(home_dir)
                            .join("powershell")
                            .join("profile.ps1"),
                        home_dir
                            .join(".config")
                            .join("powershell")
                            .join("profile.ps1"),
                    ]);
                }
            }
            // https://zsh.sourceforge.io/Doc/Release/Files.html#Files
            Shell::Zsh => {
                let zdot_dir = env::var_os("ZDOTDIR")
                    .and_then(is_absolute_dir)
                    .unwrap_or(home_dir.to_owned());

                profiles.extend([zdot_dir.join(".zprofile"), zdot_dir.join(".zshrc")]);
            }
        };

        profiles.into_iter().collect()
    }
}

fn is_absolute_dir(value: OsString) -> Option<PathBuf> {
    let dir = PathBuf::from(&value);

    if !value.is_empty() && dir.is_absolute() {
        Some(dir)
    } else {
        None
    }
}

fn get_config_dir(home_dir: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_dir)
        .unwrap_or_else(|| home_dir.join(".config"))
}
