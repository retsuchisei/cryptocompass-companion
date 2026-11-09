# CryptoCompass Companion

Records an Apex Legends custom lobby on your own machine and uploads private
recordings to [placement.gg](https://placement.gg) when the app is linked and
recording is enabled. Exact frames enter a durable local journal before upload.
The companion relays bytes; game interpretation happens on the server.

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

## Uploads and saved recordings

A server receipt confirms the highest contiguous saved frame. A socket send alone
is shown separately. After a connection/process restart, the app asks the server
where to resume and reads a bounded window from disk. Turning recording off stops
current and historical uploads. Turning it on explicitly resumes only recordings
captured under the same install credential. Relinking never silently assigns old
files to a different installation. No journal is automatically deleted after ACK.

When an older server explicitly lacks acknowledged uploads, the app labels its
socket sends as unconfirmed and keeps the recordings. Refused authentication and
conflicting data do not trigger a downgrade or delete local files. Disk/storage
failure stops recording with a visible error. The initial local retained-byte
budget is8GiB; archive recordings deliberately before exhausting local storage.
This is not a promise that an offline or destroyed computer can be recovered.

The original `.jsonl.gz` exports remain beside the `journal-v2/` directory. They
still work with the separate replay verifier. New recordings remain private to
the linked account; recording consent does not publish a live broadcast.

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
