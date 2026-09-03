# Repository instructions

## Generated screenshots and video

- Put every new simulator-generated screenshot, rendered still, poster, contact sheet, and video in
  the repository-root `captures/` directory.
- Keep `captures/` flat. Do not create subdirectories inside it.
- Use descriptive, collision-safe filenames. Include the command/run name and distinguishing seed,
  date, or frame identifier when a short filename could overwrite an existing capture.
- Treat an explicitly supplied absolute path or path with a parent directory as a deliberate user
  override. Bare output names and prefixes belong under `captures/`.
- Do not collect runtime/source textures from `assets/`, packaged artwork from `dist/`, build-cache
  previews from `target/`, or downloaded reference images from mood-board source directories.
- Preserve original artifact paths when gathering old media unless the user explicitly asks to
  remove them; documentation and audit manifests may refer to those paths.
