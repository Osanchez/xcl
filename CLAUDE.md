# Claude Code

The instructions for agents in this repository are in `AGENTS.md`, and the rules for every
contributor are in `CONTRIBUTING.md`. Both apply to you:

@AGENTS.md
@CONTRIBUTING.md

Specific to Claude Code:

- Plan a task that spans more than one subsystem or more than ~400 lines (plan mode), and propose
  how to split it into PRs before you write code.
- Run the tests and report the real output; do not stop at "the tests should pass".
- Fill in the **AI use** section of the PR description with Claude Code and your exact model name.
- For a runtime change, run `scripts/smoke.ps1` on main and on the PR's head commit and paste both
  `xcl-smoke` blocks; do not write or edit them by hand.

@docs/guide/fork-workflow-for-agents.md

## Hard limits (fork)

- **Never commit game code, game assets or anything derived from them.** No package files
  (`*.rpf`, `*.ttpf`, `*.xvc`, executables, textures, audio, video, `MicrosoftGame.Config`),
  no dumps, no disassembly or shader listings taken from a title, no console SDK headers,
  binaries or documentation, no keys, nothing for working around DRM. This applies to commits,
  branches, pull requests, issues and comments. If a change needs title data to be tested, the
  data stays on the machine and only the measurement is written down.
- **Merging a public pull request goes through `python scripts/dev/merge-pr.py <N>`**, which runs
  the security gate first. Never `git merge`, `git cherry-pick` or `gh pr merge` a public branch
  directly: a merge is followed by a build, and a build runs that branch's `build.rs`,
  procedural macros and `scripts/` as the user.
- **Commit only when asked.** Never push, open a pull request, or post an issue, review or
  comment on GitHub unless asked in this session.
- **Never report a test result, measurement or frame rate that was not produced by a real run**
  on hardware named in the report.
