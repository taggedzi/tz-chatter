# GUI coherence review and remediation

Date: 2026-09-13. Scope: current React/Tauri desktop interface at 1280x800 and the supported 900x620 minimum, plus source-level interaction wiring.

## Findings and required outcomes

| ID | Finding | Required remediation | State |
| --- | --- | --- | --- |
| G01 | Character save wrote identity, local, persona, and scene before validating generation settings, so a late error could leave a partial save. | Split the editor into independently saved Identity, Model, Persona, and Scenes sections. Validate each section before its write and report section-specific success/failure. | done |
| G02 | Vault creation/open/import/export paths were plain text fields with no native folder chooser. | Add a reusable native folder-picker control while retaining editable paths for advanced/manual use. | done |
| G03 | Empty Review Queue and Conflicts cards pushed the primary memory browser below the fold; counts were visually concatenated with headings. | Put memory browsing first, collapse empty administrative sections to compact summaries, and space headings/counts. | done |
| G04 | Character identity, portrait, model, persona, and scene editing formed one long mixed-purpose form with a single distant save action. | Add clear editor sections/tabs with local actions and a compact, stable layout at 900x620. | done |
| G05 | Regenerate/Continue/Retry and resume status were detached from the assistant response they describe. | Attach response actions to the latest assistant message and render resume/error feedback as a deliberate status region near the composer. | done |
| G06 | Initiative controls remained actionable without a character/session, and deleting a local had no confirmation despite potentially breaking referenced sessions. | Disable unavailable initiative controls with an explanatory empty state; confirm local deletion and explain its consequence. | done |
| G07 | Emoji and character-switcher composite widgets used incomplete listbox/tab semantics and did not expose keyboard highlight through `aria-activedescendant`. | Use valid option/tab relationships and expose active option identity to assistive technology. | done |
| G08 | The Character editor's New scene action produced an empty id, but its save handler required an id before the id-generation step, so the control could never save. | Let the bounded title-to-id step run before persistence and prove the UI reaches `local_save`. | done |
| G09 | Opening the character switcher could leave the emoji dialog rendered underneath it; an initial dismissal implementation also triggered a React render-phase update warning. | Make chat popovers mutually exclusive and dispatch dismissal outside React state updater callbacks. | done |

## Evidence baseline

- Current browser-rendered React UI inspected with synthetic Tauri IPC at 1280x800 and 900x620.
- `npm run test:unit`, `npm run lint`, `npm run typecheck`, and `npm run build` passed before remediation.
- Frontend invoke names were compared with the Rust `generate_handler!` registration list; no missing command registration was found.
- Native Windows computer-use initialization failed at the host sandbox helper, so tray, notification, native dialog, DPI, and screen-reader behavior were not certified by the review.

## Completion evidence

- G01/G04: the Character editor now has four labelled tabs with independent Identity, Model, Persona, and Scenes save actions. Nine focused tests cover inheritance and rejection of invalid temperature/max-token input before generation persistence.
- G02: Tauri's dialog plugin is initialized and permitted; Character New/Add, Settings Vault, pack source/import, and export/restore parent selection use `FolderField`. Editable paths remain available. New destinations append a tested platform-appropriate child path.
- G03: Memory browse/edit is before Review Queue and Conflicts. Zero-count sections are compact. At 900x620, measured list→editor and editor→queue overlap are both false.
- G05: the render harness finds `.response-actions` inside the latest assistant message. Resume/error feedback is a distinct composer-adjacent status region.
- G06: focused tests cover character/session availability; controls expose disabled states and guidance. Scene deletion uses a native warning confirmation naming the affected file and session consequence.
- G07: emoji categories/results use valid button-group semantics; the switcher combobox owns its listbox and exposes `aria-activedescendant=character-choice-0` in the fixture.
- G08/G09: the rendered New scene journey invokes `local_save`; opening the switcher leaves no stacked emoji picker and the clean run records zero console/runtime errors.
- `docs/review/gui_remediation_review.mjs` PASS. Machine-readable evidence is `docs/review/gui-remediation-results.json`; current screenshots use the `gui-*.png` names beside it.
- `npm run test:unit` PASS (42), `npm run lint` PASS, `npm run typecheck` PASS, `npm run build` PASS.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` PASS (172 passed, 2 ignored), Rust fmt check PASS, strict Clippy PASS.
- `tauri build --no-bundle` PASS; the dialog-linked Windows executable was produced at `src-tauri/target/release/tz-chatter.exe`.

Native Windows computer-use still fails at the host sandbox helper. The dialog plugin is compile/link verified and the buttons are browser-render verified, but an actual native folder dialog click, screen-reader behavior, DPI scaling, tray, and notifications remain outside this evidence.
