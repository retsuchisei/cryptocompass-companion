# CryptoCompass Companion

Records an Apex Legends custom match on your own machine and sends it to
[cryptocompass.tech](https://cryptocompass.tech), where it becomes ring data.
It receives Apex's LiveAPI stream on localhost, writes every frame to a session
log on your disk, and forwards it upstream.

Windows only, and that is not a gap to fill: Apex is a Windows game and LiveAPI
does not exist on consoles.

## Two warnings, before you install anything

**It does nothing in pubs or ranked.** LiveAPI has to be allowed by the server
your game connects to, and public matchmaking does not allow it. A client in a
ranked match sends one `init` event and then nothing at all, for the whole
match - that is measured, not guessed. This app is useful in custom lobbies and
nowhere else.

**The installer is unsigned, so Windows will interrupt you.** Buying an
Authenticode certificate is money and paperwork for a build a handful of people
run, so it is deferred. Expect the browser to discourage the download and
SmartScreen to interrupt the first run. Every release publishes a SHA-256
checksum so you can confirm you got what we built.

## What it records, and who is in it

The LiveAPI stream describes every player in the lobby, not only you.

That is why recording is off when the app starts, off again after every update,
and why one screen says all of this before the switch can be turned on. Nothing
is recorded or sent until somebody presses that button.

## Building it

```bash
npm install
npm run check   # the gate: typecheck, node --test, cargo fmt, clippy, cargo test, vite build
npm run dev     # the app, against the dev interface
```

Node 24 and a stable Rust toolchain with `rustfmt` and `clippy`. The same gate
runs in CI on a Windows runner.

## The design

The app's rules are in `CLAUDE.md`: it relays rather than parses, the local file
is written before the network is touched, and nothing is recorded or sent until
somebody turns it on.
