# Stage 10 frontend fidelity

## Artifacts

- Wide concept: `docs/design/stage10-shell-wide.png`
- Compact concept: `docs/design/stage10-shell-compact.png`
- Wide implementation at 1440 × 900:
  `docs/design/stage10-shell-wide-rendered.png`
- Compact implementation at 820 × 760:
  `docs/design/stage10-shell-compact-rendered.png`

The concepts were generated before implementation and used as visual references.
The rendered application was exercised with Playwright CLI because an interactive
browser surface was not available in this environment. Both viewports were then
inspected at native resolution.

## Comparison

| Area           | Concept                                                                    | Implementation                                                                      | Result  |
| -------------- | -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ------- |
| Navigation     | Compact Foldry mark, Tasks/Profiles, connection, locale and theme controls | Same information architecture and control order                                     | Matched |
| Wide layout    | Folder browser, task list, inspector and persistent command bar            | Three columns plus the fixed command bar at 1440 × 900                              | Matched |
| Compact layout | Narrow rails preserve access to browser and inspector                      | Drawer-opening rails keep the task list readable at 820 × 760                       | Matched |
| Typography     | Graphite, compact sans-serif hierarchy, monospaced paths                   | Mantine tokens use the same hierarchy and monospace paths                           | Matched |
| Palette        | Near-white surfaces, graphite copy, cobalt interaction color               | Light theme follows that palette; dark/system theme uses equivalent semantic tokens | Matched |
| Task controls  | Status, progress, and run controls remain visible per task                 | Controls are keyboard reachable and remain visible in both tested layouts           | Matched |
| System states  | Connection and task/run state are not color-only                           | Icons, labels, progress values and status text accompany color                      | Matched |

## Copy and intentional deviations

There is no unapproved marketing copy above the fold. Task titles are derived from
the source-directory basename because the version 1 task contract has no separate
display-name field; therefore the fixture produces two tasks named `Work`.

The first wide concept accidentally included delete-source and schedule controls.
They are deliberately absent because neither behavior belongs to the accepted
version 1 contracts. Compact cards stack their status and controls instead of
reproducing the wide card grid, preserving legibility and interaction targets.
The folder browser shows bootstrap roots only at this stage; lazy expansion and
filesystem-state indicators are part of stage 12.

## Verification

- The Tasks and Profiles routes were exercised.
- The folder drawer was opened and closed in compact mode.
- English/Russian and light/dark controls are covered by component tests.
- Accessibility snapshots expose named navigation, buttons, form controls,
  progress bars and landmarks.
- Browser console result: 0 errors, 0 warnings.
