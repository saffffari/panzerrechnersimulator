# Repository instructions

## Workspace separation

- Keep `/home/alex/panzer` exclusively for this game and its game assets, transfers, and archives.
- Do not create links to unrelated work projects, clinical/research datasets, or general storage
  archives in this workspace. Check explicit destinations before creating bulk symlinks.
- Asset discovery must stay within the game's documented source locations; do not follow
  unrelated external dataset links while searching for game resources.

## Generated screenshots and video

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
