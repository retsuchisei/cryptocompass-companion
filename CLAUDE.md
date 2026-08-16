# cryptocompass-companion

A Windows desktop app that receives Apex's LiveAPI stream on localhost, writes
it to disk, forwards it to our collector, and updates itself. Tauri v2: the Rust
core under `src-tauri/`, the SolidJS interface at the root.

The design and every argument behind it live in the private `apex-guideline`
repository, at `docs/superpowers/specs/2026-08-16-companion-app-design.md`. Read
it before changing anything structural. Nothing here repeats its reasoning.

This repository is public and stands alone. It is not a submodule of
`apex-guideline` and must not import from it - the two are kept side by side by
hand.

## Commands

```bash
npm run check   # the gate: typecheck, node --test, cargo fmt, clippy, cargo test, vite build
npm run test    # the interface's tests alone
npm run dev     # tauri dev
npm run build   # tauri build
```

The interface's tests run on Node's own test runner - no framework, because
Node 24 executes TypeScript directly. They cover the pure modules; components
are not tested, which would need a DOM harness nobody has decided on. A green
gate means the rules are right, not that the screen is.

`check` is the single gate. Run it before claiming anything works, and show its
output.

`dev` and `build` are Tauri. The bare Vite halves are `vite:dev` and
`vite:build`, and `tauri.conf.json` calls them by those names; renaming either
back to `dev` or `build` makes `tauri dev` invoke itself forever.

## The rules

**This app relays. It does not parse.** Frames are stored and forwarded byte for
byte. Reading the `category` field of a JSON frame to recognise `init` is the
whole of the permitted interpretation; ring, player and match semantics belong
to the server, in `apex-guideline`'s `scripts/rings/adapters/liveapi.py`. A
parser here would mean a schema change needs a release, and a release needs
every tester to update before the data is usable again.

It follows that the interface reports counts, bytes and the time of the last
frame - never "ring 3 is closing".

**The session log format is not ours to invent.** It is
`services/liveapi/session.py` in `apex-guideline`, byte for byte, so that
`services/liveapi/replay.py` verifies this app unmodified. Changing the header
line, the row shape or the `.jsonl.gz` name on close breaks that verifier, which
is the only thing that tests the recorder without the game.

**Recording is off on first run and after every update.** The stream describes
every player in the lobby and not only the person running the app, so turning it
on is a deliberate act by the person at the keyboard. No code path may start
recording or send anything upstream on its own.

**JSON text frames, not protobuf.** The config the app writes sets
`cl_liveapi_use_protobuf` to false and the whole stack downstream assumes it. A
binary frame is recorded as malformed rather than decoded.

**The local file is written before the network is touched.** A closed laptop, a
dropped link or a collector that is down must all cost nothing: the match lands
on disk first and catches up afterwards.

**The game's `config.json` is not ours either.** `src-tauri/src/config.rs`
writes the same file as `scripts/liveapi.ps1` in `apex-guideline`, so a player
who ran the script and a player who ran the app leave the game in the same
state; edit one and the other is wrong. A config already there is somebody's
setup until it says `apexRankedManaged`, and then it is kept, not replaced.

The launch option `+cl_liveapi_enabled 1` is the one thing the app does not
write. It lives in the Steam or EA client's own store, which rewrites the file
underneath anything that edits it, so `Setup.tsx` asks for one paste and then
proves it took by waiting for a frame. Claiming success without one would be
worse than asking.

**A tag is the whole trigger.** Pushing `v<version>` builds on a Windows runner
and publishes the installer, its updater signature, `latest.json` and
`SHA256SUMS`. Nothing else releases, and nothing releases by hand.

One version is declared in three manifests - `package.json`,
`src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` - and they are bumped
together. Each is read by something different: the installer's name, the
manifest the updater compares against, and `CARGO_PKG_VERSION`, which is what
consent is stored against, so a bump that reaches two of the three ships an
update that is either never offered or never asks again. `npm run check` fails
when they disagree and the release workflow checks the tag against them.

**The updater's public key is compiled into every installed copy**, so
replacing the key orphans every install: they stop accepting updates and have
to be reinstalled by hand. It is `plugins.updater.pubkey` in
`tauri.conf.json`; the private half is the repository's only Actions secret.

## Layering

The Rust side keeps its logic in functions that take bytes and return bytes, so
the parts worth testing need neither a window nor a game. `main.rs` is wiring
only. Anything that needs a running app to exercise is a sign the logic is in
the wrong place.

The interface holds no protocol logic. It shows state and owns consent.

## Language

Comments, commit messages, developer-facing strings and every identifier are
English and ASCII, the same rule `apex-guideline` runs. The one exception is
text a user reads: the consent screen has to speak the user's language. There is
no message catalogue yet, so that text sits where it is displayed.
