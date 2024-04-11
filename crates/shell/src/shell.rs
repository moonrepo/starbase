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
    pub fn get_profile_locations(&self, home_dir: &Path) {}
}
