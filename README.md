<img src="docs/logo/tome-icon.png" width="96" alt="">

# TOME

**Track, Own, Merge, Explore — a desktop client for Lore.**

[한국어](README.ko.md)

TOME is a community desktop client for [Lore](https://github.com/EpicGames/lore), the version control system by Epic Games. It talks to Lore in-process through Lore's C API, so it never parses command-line output.

> **Status:** planning. Nothing is usable yet.

## Four things it does

- **Track** — a Smartlog of your work: local draft stacks against the remote latest, public history folded away, with revision numbers along each branch's first-parent chain and a full graph when you want it.
- **Own** — locks for the whole team on one board: who is working on which asset, start work (lock) and submit in one step, live updates when locks change.
- **Merge** — restack drafts with a preview before anything moves: which files merge on their own, which binary files conflict, which locks would be broken.
- **Explore** — a View and Hydration manager: choose what to materialize from a checkbox tree, see the `.lore/view` globs and the disk size before you apply, with presets per role.

Two modes share one core: a **programmer mode** (stacks and the graph) and an **artist mode** (an asset grid, no graph).

## Not an official product

TOME is an independent project. It is not made, endorsed or supported by Epic Games. "Lore" is a trademark of Epic Games, Inc. and is used here only to say what TOME works with. Lore's MIT license covers its code, not its name.

## Documents

- [Architecture and stack decision](docs/architecture.md)
- [Planning handoff (2026-10-02)](docs/handoff.md)
