//! Embedded, opt-in samples. All writes use the validated portable-pack importer.
use crate::{
    characters::{self, CharacterError, CharacterLibraryItem},
    portability,
};
use std::{fs, path::Path};

const HOLMES_FILES: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!("../../examples/characters/sherlock-holmes/manifest.json"),
    ),
    (
        "character.md",
        include_bytes!("../../examples/characters/sherlock-holmes/character.md"),
    ),
    (
        "persona.md",
        include_bytes!("../../examples/characters/sherlock-holmes/persona.md"),
    ),
    (
        "scene.md",
        include_bytes!("../../examples/characters/sherlock-holmes/scene.md"),
    ),
    (
        "locals/baker-street.md",
        include_bytes!("../../examples/characters/sherlock-holmes/locals/baker-street.md"),
    ),
    (
        "locals/fireside.md",
        include_bytes!("../../examples/characters/sherlock-holmes/locals/fireside.md"),
    ),
    (
        "locals/correspondence.md",
        include_bytes!("../../examples/characters/sherlock-holmes/locals/correspondence.md"),
    ),
    (
        "memories/semantic/holmes-origins.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-origins.md"
        ),
    ),
    (
        "memories/relationships/holmes-watson.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/relationships/holmes-watson.md"
        ),
    ),
    (
        "memories/people/holmes-irene-adler.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/people/holmes-irene-adler.md"
        ),
    ),
    (
        "memories/people/holmes-mycroft.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/people/holmes-mycroft.md"
        ),
    ),
    (
        "memories/people/holmes-hudson.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/people/holmes-hudson.md"
        ),
    ),
    (
        "memories/relationships/holmes-lestrade.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/relationships/holmes-lestrade.md"
        ),
    ),
    (
        "memories/semantic/holmes-music.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-music.md"
        ),
    ),
    (
        "memories/semantic/holmes-restlessness.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-restlessness.md"
        ),
    ),
    (
        "memories/semantic/holmes-rooms.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-rooms.md"
        ),
    ),
    (
        "memories/semantic/holmes-seventeen-steps.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-seventeen-steps.md"
        ),
    ),
    (
        "memories/episodic/holmes-norbury.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/episodic/holmes-norbury.md"
        ),
    ),
    (
        "memories/episodic/holmes-silver-blaze.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/episodic/holmes-silver-blaze.md"
        ),
    ),
    (
        "memories/episodic/holmes-blue-carbuncle.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/episodic/holmes-blue-carbuncle.md"
        ),
    ),
    (
        "memories/people/holmes-moriarty.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/people/holmes-moriarty.md"
        ),
    ),
    (
        "memories/semantic/holmes-return.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-return.md"
        ),
    ),
    (
        "memories/semantic/holmes-pack-provenance.md",
        include_bytes!(
            "../../examples/characters/sherlock-holmes/memories/semantic/holmes-pack-provenance.md"
        ),
    ),
    (
        "assets/portrait.png",
        include_bytes!("../../examples/characters/sherlock-holmes/assets/portrait.png"),
    ),
];

/// Install into a new child of the user-selected parent. Never update an existing vault.
pub fn install_holmes(
    library_path: &Path,
    parent_dir: &Path,
) -> Result<CharacterLibraryItem, CharacterError> {
    if parent_dir.as_os_str().is_empty() {
        return Err(CharacterError::Invalid(
            "Choose a parent folder for Sherlock Holmes.".into(),
        ));
    }
    fs::create_dir_all(parent_dir)?;
    let parent = fs::canonicalize(parent_dir)?;
    let destination = parent.join("sherlock-holmes");
    match fs::symlink_metadata(&destination) {
        Ok(_) => return Err(CharacterError::Invalid("A sherlock-holmes folder already exists here. Add that existing folder to Characters, or choose a different parent folder.".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    // Unique, app-created directory; never a user-provided deletion target.
    let staging = parent.join(format!(
        ".holmes-bundled-{}",
        crate::storage::new_stable_id()
    ));
    fs::create_dir(&staging)?;
    let result = (|| {
        for (relative, bytes) in HOLMES_FILES {
            let file = staging.join(relative);
            if let Some(directory) = file.parent() {
                fs::create_dir_all(directory)?;
            }
            fs::write(file, bytes)?;
        }
        portability::import_pack(&staging, &destination, false)
            .map_err(|error| CharacterError::Storage(error.to_string()))?;
        characters::add_existing(library_path, &destination).map_err(|error| {
            CharacterError::Persistence(format!("Holmes was installed at {} but could not be added to the library: {error}. Use Characters > Add with that folder to recover.", destination.display()))
        })
    })();
    let cleanup = fs::remove_dir_all(&staging);
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(CharacterError::Persistence(format!("Holmes was installed, but temporary pack cleanup failed at {}: {error}. The installed character is available in Characters.", staging.display()))),
        (Ok(item), Ok(())) => Ok(item),
    }
}
