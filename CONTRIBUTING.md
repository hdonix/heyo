# Contributing to Heyo

Building, running and the tests are in the [README](README.md#development).

Heyo has not had a release yet. Until it has, nothing is kept working for
the sake of what an earlier commit did: names, formats and interfaces
change in place, without code that reads the old ones.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

    type(scope): subject

    Why the change was needed, and what it does that the subject does not
    say. Wrapped at 72 characters.

**type** is one of

| type | for |
|---|---|
| `feat` | something Heyo, an extension or a developer can now do |
| `fix` | something that behaved wrongly and now does not |
| `refactor` | the same behaviour, arranged differently |
| `perf` | the same behaviour, faster or lighter |
| `test` | tests alone |
| `docs` | documentation alone |
| `build` | dependencies, bundling, packaging |
| `ci` | the workflows under `.github/` |
| `chore` | what fits nowhere above (the repository's own upkeep) |
| `revert` | undoing an earlier commit |

**scope** says where, when one place is meant, and is left out when the
change is across several:

| scope | what |
|---|---|
| `host` | the Rust program, `src-tauri/` |
| `platform` | what talks to a desktop, `src-tauri/src/platform/` |
| `web` | the interface in the web view, `src/` |
| `runtime` | the Node.js process extensions run in, `runtime/` |
| `api` | `@heyo/api` as extensions see it, `packages/api/` and `runtime/src/heyo/` |
| `dev` | `heyo build`, `dev`, `migrate`, `ext check` |
| `gnome` | the GNOME Shell extension |
| `packaging` | `packaging/`, `scripts/install.sh` |
| `compat` | the compatibility run, `scripts/compat/` |

**subject** says what the commit does, in the imperative, in at most 72
characters with the type and scope, without a capital at the start unless
it is a name, and without a full stop. A `!` before the colon marks a
change that an extension written for an earlier commit would notice.

One commit does one thing. A fix found while doing something else is a
commit of its own.

`scripts/check-commits.sh` checks the first line, in CI for every push and
pull request. To have it checked as you commit:

    git config core.hooksPath .githooks
