# Repository instructions

## Workspace separation

- Keep `/home/alex/panzer` exclusively for this game and its game assets, transfers, and archives.
- Do not create links to unrelated work projects, clinical/research datasets, or general storage
  archives in this workspace. Check explicit destinations before creating bulk symlinks.
- Asset discovery must stay within the game's documented source locations; do not follow
  unrelated external dataset links while searching for game resources.

## Generated screenshots and video

- **Owner's browsing folders (Windows, 2026-10-28): every screenshot ends up in
  `C:\shop\panzerrechnersystem\captures\screenshots\` and every video in
  `C:\shop\panzerrechnersystem\captures\videos\`, the main checkout's folders, even when the work
  happens in a worktree. Both folders stay flat: never create a subfolder in them.** The owner
  clicks through the whole set sorted by date modified.
  - From a worktree, prefix filenames with the worktree/branch name (for example
    `scatter-treefall_day_seed7.png`) so every file is identifiable in the shared folder.
  - Harnesses that write into their own `captures/` or `artifacts/` trees keep working unchanged.
    After producing media, run `powershell -ExecutionPolicy Bypass -File tools\sync_captures.ps1`
    (from any checkout). It copies every checkout's captures and artifacts media into the two
    folders, flattens names to `<worktree>-<subfolders>-<file>`, keeps each file's timestamp, and
    skips files already copied. Originals stay where tests and manifests expect them.
  - Writing straight into the two folders by absolute path is also fine for new one-off captures.
  - `captures/` is git-ignored: nothing in these folders is ever committed or uploaded.
- Put simulator-generated screenshots, rendered stills, posters, contact sheets, and videos in
  the repository-root `captures/` directory, except Astra look-development captures below.
- The user requested `captures/astra/` for new Astra look-development captures. Use an explicit
  output path under that directory; keep it flat. Other capture series remain flat in `captures/`.
- Use descriptive, collision-safe filenames. Include the command/run name and distinguishing seed,
  date, or frame identifier when a short filename could overwrite an existing capture.
- Treat an explicitly supplied absolute path or path with a parent directory as a deliberate user
  override. Bare output names and prefixes belong under `captures/`.
- Do not collect runtime/source textures from `assets/`, packaged artwork from `dist/`, build-cache
  previews from `target/`, or downloaded reference images from mood-board source directories.
- Preserve original artifact paths when gathering old media unless the user explicitly asks to
  remove them; documentation and audit manifests may refer to those paths.
