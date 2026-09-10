# First-person media library

Open `captures/astra/first_person_library_20260905.html` locally. The page contains
native video controls with sound, search and area filters, expandable earlier or
failed evidence, and the complete 54-row mechanic ledger. Playback never starts
automatically. Videos use `preload="none"`; starting one pauses the others.
There are no remote assets, external scripts, audio-only links, new renders, or
audio processing in this tool.

From the repository root:

```sh
python tools/astra_first_person_library.py build
python -m unittest discover -s tools -p test_astra_first_person_library.py -v
```

The build reads every `artifacts/astra-first-person-*/queue.json`, actual
`captures/astra/*_manifest.json` with the first-person capture schema, the saved
scenario definitions, and the coverage ledger. A final MP4 can be recovered from
its receipt even if a failed queue omitted its links. Active recordings and
`_video_only.mp4` intermediates are excluded. Earlier files are left in place.
The generated HTML/index may be refreshed with the same command; use
`build --stem NAME` to preserve a separate library snapshot.

The index is `captures/astra/first_person_library_20260905.json`. The separate,
append-only review sidecar is `captures/astra/first_person_qa_20260905.json`.
Each review binds to the actual final-video SHA-256, capture-receipt SHA-256, and
scenario ID. Replacing a video or editing its receipt leaves the old review
unbound; it cannot approve the new bytes. New library builds do not change QA.

To append a real review after inspecting the exact clip:

```sh
python tools/astra_first_person_library.py review \
  --manifest captures/astra/EXACT_CAPTURE_manifest.json \
  --visual pass --audio pending --art pending \
  --reviewer 'Reviewer name' --evidence 'contact-sheet review' \
  --mechanic commander \
  --note 'Describe exactly which behavior was visible and what remains unreviewed.'
```

Use `--audio pass` only after listening to the soundtrack. Repeat `--mechanic`
for each ledger ID actually covered. A shared scenario does not imply approval
of all related mechanics. The review command checks ledger IDs but cannot judge
the evidence for a reviewer. The last matching review supersedes earlier
decisions while retaining them in the sidecar.

Five independent results are shown: scenario state checks, encoded streams,
visual coverage, audio quality, and art finish. No outcome assertions means
"pending", rather than a fabricated pass or failure. A fully reviewed mechanic
needs a matching, explicitly scoped visual/audio approval plus passing actual
scenario and stream checks. Art finish stays separate from mechanic coverage.
Preferred clips favor those approvals, then visual-only approval, then unreviewed
passing evidence. A newer unreviewed clip cannot displace an earlier reviewed
passing clip. Alternate and failed versions remain accessible.

The initial sidecar records the root agent's reported contact-sheet and receipt
reviews, with attribution. Commander, P08 and Panzerfaust handling have scoped
visual checks; Panzerfaust art is explicitly unpolished. The old blocked driver
views, out-of-frame extinguishing target, failed gun firing, and dark Playtest
garage/grove/refuel images are rejected for visual coverage. The revised driver
geometry probe still needs exterior-readability review. No soundtrack has been
approved by this curation step. Manual reload's failed attempt remains listed;
its unfinished silent intermediate is not presented as a completed video.

These captures are frozen evidence of their recorded builds. Neither a stream
check nor the library's presence establishes current-build behavior, adequate
audio quality, historical accuracy, or finished art.
