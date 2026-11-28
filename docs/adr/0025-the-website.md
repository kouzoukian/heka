# A static website that holds the user manual

`site/` is the website at `https://heka.kouzoukian.dev`: a landing page that gets heka installed, and the user manual. It is a Next.js app with Fumadocs, built into static files and published to GitHub Pages by `.github/workflows/site.yml` from `main`. Pull requests only check and build it.

## The manual

- `site/content/docs` is the only copy of the user manual. `README.md` keeps what someone needs to install and build heka, because it ships in the release zip, and links to the site.
- A merged change to the manual is published at once, which can be shortly before the release that has the behaviour it describes.
- The install steps are told three times: in `README.md`, in `site/content/docs/installation.mdx` and, shortened, on the landing page (`site/app/(home)/page.tsx`). Change them together.
- Search is built into the files and runs in the browser.
- The colours are those of the app's themes and the font is the app's, copied with its license. The screenshots in `site/public/screenshots` are of the window over made-up data, dark and light.

## Download

- Releases stay the only way heka is distributed ([ADR 0004](0004-pinned-toolchain-ci-and-release.md)).
- The download button asks GitHub for the latest release from the visitor's browser, so publishing a release needs no rebuild of the site. As in [ADR 0020](0020-checking-for-a-new-release.md), the address of the zip is built from the repository and a tag that is a version, so nothing in the answer chooses where it points.
- Until GitHub answers, and without JavaScript, the button leads to the latest release's page. It does the same when the request is refused (GitHub limits unauthenticated requests for each address), fails, or finds no zip.

## No comments, no analytics

- [ADR 0003](0003-no-code-comments.md) covers the site. `heka-lint` reads only Rust, TOML, YAML, `.gitignore` and `.hex` files, so `site/scripts/check-comments.ts` checks the site's TypeScript, JavaScript, CSS and MDX with their own parsers, which tell a comment from `//` in a string or in text. MDX holds JSX and expressions, so unlike Markdown it counts as code.
- TypeScript stays on 6.x, the last line with the compiler API the check uses.
- The site carries no analytics and loads nothing from other hosts but that request.
