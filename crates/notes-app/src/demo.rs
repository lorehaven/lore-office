//! The note a new vault starts with.

/// A short tour of the app, in the app's own format, so it also shows what a note and a deck look like.
#[must_use]
pub fn welcome() -> String {
    "---\ntheme: default\n---\n# Welcome to lore-notes\n\nNotes are plain markdown files that stay on your machine. Write on the left, read on the right.\n\n- Link notes with `[[double brackets]]`, such as [[Ideas]].\n- Tag them with #welcome.\n- Every note is also a deck.\n\n---\n\n## A second slide\n\n> [!tip] Present it\n> Slides are split by a line with three dashes. Press **Present** (or F5) to try it.\n\n- [x] open lore-notes\n- [ ] write something\n\n---\n\n## A third slide\n\nSlides can hold lists, tables, code and math.\n\n| Shape | Sides |\n|---|---|\n| Triangle | 3 |\n| Square | 4 |\n\n$$E = mc^2$$\n\n???\nSpeaker notes follow a line with three question marks. Only you see them.\n"
        .to_string()
}
