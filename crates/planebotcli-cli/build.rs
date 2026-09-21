use std::path::PathBuf;

/// Make cargo rebuild `planebotcli-cli` whenever the vendored `skill/SKILL.md`
/// at the workspace root changes. The path is relative to this crate's
/// manifest dir (`crates/planebotcli-cli/`); `../../skill/SKILL.md` lands at
/// the workspace's `skill/SKILL.md`. The same path is then resolved against
/// the source tree so a missing file fails the build loudly (vendoring a
/// missing skill would silently bake in an empty SKILL.md and leave
/// `include_str!` happy with a zero-length string).
fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let skill_path = manifest_dir
        .join("..")
        .join("..")
        .join("skill")
        .join("SKILL.md");
    println!("cargo:rerun-if-changed={}", skill_path.display());
    if !skill_path.exists() {
        panic!(
            "planebotcli-cli build.rs: expected {} to exist; \
             the skill is the contract an AI agent reads before \
             answering anything else, so vendoring a missing file \
             is a hard error rather than a silent zero-length include_str!",
            skill_path.display()
        );
    }
}
