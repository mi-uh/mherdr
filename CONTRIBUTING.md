# Contributing to mherdr

mherdr is a personal fork of [herdr](https://github.com/herdrdev/herdr). It
carries a small set of performance and navigation changes on top of upstream
and is rebased onto upstream regularly.

- **Bugs in mherdr's own changes** (rendering, frame rate, space/agent
  navigation, anything listed in [MHERDR.md](MHERDR.md)): open an issue here
  with the mherdr commit (`git rev-parse --short HEAD`), your OS and terminal,
  and the shortest reproduction you have.
- **Bugs that also happen in upstream herdr**: report them
  [upstream](https://github.com/herdrdev/herdr), following its contribution
  rules.
- **Pull requests** are welcome for small, focused fixes and measurable
  performance work. Keep commits in lowercase conventional style
  (`perf: …`, `fix: …`), run `just check`, and include before/after numbers
  from `scripts/mherdr_bench.py` for performance changes.

Larger features that are not about responsiveness or navigation probably
belong upstream instead.
