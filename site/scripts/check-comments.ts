import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { commentsIn } from './comments.ts';

const site = resolve(import.meta.dirname, '..');

const files = execFileSync('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'], {
  cwd: site,
  encoding: 'utf8',
})
  .split('\0')
  .filter((path) => path && existsSync(join(site, path)));

const found = files.flatMap((path) =>
  commentsIn(path, readFileSync(join(site, path), 'utf8')).map(
    ({ line, text }) => `site/${path}:${line}: ${text}`,
  ),
);

if (found.length > 0) {
  console.error(`Code carries no comments (docs/adr/0003-no-code-comments.md):\n${found.join('\n')}`);
  process.exitCode = 1;
}
