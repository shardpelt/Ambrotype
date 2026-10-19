# Security

Ambrotype runs plugins, downloads AI models, layouts and presets from the internet, and reads files from cameras and other apps, so security problems matter. Thanks for reporting them.

## Reporting a problem

Please **don't open a public issue.** Report it privately through GitHub: [Security → Report a vulnerability](https://github.com/shardpelt/Ambrotype/security/advisories/new). Include what you found, how to reproduce it, and what it lets an attacker do.

You'll get a reply within a week. Once it's fixed, the advisory is published with credit to you, unless you'd rather not be named.

## What's in scope

- A plugin escaping its sandbox (more than its permissions allow), or a plugin running without the user's approval.
- A crafted photo, sidecar, preset, layout, theme, LUT, collage or catalogue that crashes the app in a way that could be exploited, or reads or writes files it shouldn't.
- Downloads (AI models, hub items, the lens database) that could be swapped without the checksum check catching it.
- API keys for the assistant leaking from the system keychain or into logs.

## Supported versions

There are no releases yet; fixes go into `main`.
