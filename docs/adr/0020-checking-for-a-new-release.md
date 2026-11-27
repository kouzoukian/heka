# heka asks GitHub for the latest release and downloads nothing

The one thing heka sends off the PC is a request for its latest release, so that a player learns of a newer version. It is on by default and has a switch in app settings.

## What is sent

- `GET https://api.github.com/repos/kouzoukian/heka/releases/latest`, with `Accept: application/vnd.github+json` and `User-Agent: heka/<version>`. Beyond those, only what HTTP and TLS need (`Host`, `Connection`) is sent. A test reads the request as a local server receives it. GitHub sees the player's IP address, as with any request.
- Cookies and authentication are disabled, so nothing is kept between checks and no Windows credentials are offered, to a proxy either. A proxy that asks for them fails the check.
- Redirects are followed, as WinHTTP does by default, which never goes from HTTPS to HTTP. GitHub redirects the address of a repository that was renamed, and copies already out would otherwise stop learning of releases.
- No `X-GitHub-Api-Version`: a pinned version that GitHub retires would end the checks of every copy already out.

## WinHTTP

`heka-core` reaches the web through the `Web` trait; `heka-windows` implements it with WinHTTP, which uses Windows' certificates and proxy settings and adds no crate to the build. Resolving, connecting, sending and each receive time out after 10 seconds, with WinHTTP's own limit on the wait for the response headers, and an answer over 1 MiB is refused.

## Which release counts

- GitHub's latest release is the newest that is neither a draft nor a pre-release. heka still refuses either flag, and a tag with a hyphen, which the release workflow publishes as a pre-release ([ADR 0004](0004-pinned-toolchain-ci-and-release.md)).
- A tag is `v` and three numbers, compared as numbers. A running pre-release is older than the release with its numbers.
- The window links to the release's page, built from the repository's address and the version, so nothing in the answer chooses where the link goes.

## When

- At start, then 24 hours after a check that succeeded and 1 hour after one that failed, so that a PC whose network is late at logon still learns of a release the same day. Turning the switch on checks at once if one is due.
- A thread of its own checks, so a slow answer holds up neither input nor the window. It wakes each minute to compare times, so that time spent asleep counts. It is not joined at quit.
- A failure goes to the log and nowhere else, when it appears or changes rather than at every retry. A release already found stays shown through a failed check; with the switch off, none is shown.

## Unsafe code

Each `unsafe` call is sound because: every string passed to WinHTTP is null-terminated UTF-16 that outlives the call, except the headers, whose length is passed; each handle is checked for null, owned by one value and closed once, a request before its connection and its connection before its session; the options, status and read calls write to or read from buffers heka owns, at most the length passed; the session is synchronous, so no call keeps a pointer after it returns.
