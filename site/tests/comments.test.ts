import assert from 'node:assert/strict';
import { test } from 'node:test';
import { commentsIn } from '../scripts/comments.ts';

const found = (path: string, ...lines: string[]) =>
  commentsIn(path, lines.join('\n')).map(({ line, text }) => `${line}: ${text}`);

test('a line comment in TypeScript is found with its line', () => {
  assert.deepEqual(found('a.ts', 'const a = 1;', 'const b = 2; // two'), ['2: // two']);
});

test('slashes inside strings, templates, regexes and JSX text are not comments', () => {
  assert.deepEqual(
    found(
      'a.tsx',
      'const url = "https://example.com";',
      'const glob = `src/**/*.ts ${url} // here`;',
      'const slash = /\\/*[/]*/;',
      'export const A = () => <p>https://example.com /* shown */</p>;',
      'export const B = () => (',
      '  <p>',
      '    // shown too',
      '  </p>',
      ');',
    ),
    [],
  );
});

test('comments are found wherever TypeScript allows them', () => {
  assert.deepEqual(
    found(
      'a.tsx',
      '/// <reference types="node" />',
      '/** documented */',
      'export const a = f(/* inline */ 1);',
      'export const b = `x ${a /* in a template */}`;',
      'export const C = () => (',
      '  <p /* on a tag */>',
      '    {/* between children */}',
      '    text {a /* after a value */}',
      '  </p>',
      ');',
      'function g() {',
      '  // alone in a body',
      '}',
      '// last',
    ),
    [
      '1: /// <reference types="node" />',
      '2: /** documented */',
      '3: /* inline */',
      '4: /* in a template */',
      '6: /* on a tag */',
      '7: /* between children */',
      '8: /* after a value */',
      '12: // alone in a body',
      '14: // last',
    ],
  );
});

test('JavaScript files are read too', () => {
  assert.deepEqual(found('next.config.mjs', "/** @type {number} */", 'export default 1;'), [
    '1: /** @type {number} */',
  ]);
});

test('CSS comments are found, and slashes in strings and urls are left alone', () => {
  assert.deepEqual(
    found(
      'a.css',
      "@import 'tailwindcss';",
      'a { background: url(https://example.com/*.png); content: "/* shown */"; }',
      '/* one */',
      'b { margin: 0 /* two */ 1px; }',
    ),
    ['3: /* one */', '4: /* two */'],
  );
});

test('MDX comments are found in text, between blocks, in attributes and in modules', () => {
  assert.deepEqual(
    found(
      'a.mdx',
      '---',
      'title: A',
      '---',
      '',
      'import x from "./x"; // module',
      '',
      '{/* block */}',
      '',
      'Words {/* inline */} words.',
      '',
      '<Shot',
      '  width={1 /* attribute */}',
      '  {...x /* spread */}',
      '/>',
    ),
    ['5: // module', '7: /* block */', '9: /* inline */', '12: /* attribute */', '13: /* spread */'],
  );
});

test('code samples, links and quoted hashes in MDX are not comments', () => {
  assert.deepEqual(
    found(
      'a.mdx',
      '---',
      'title: "Item #1"',
      'description: C# and https://example.com/#top',
      '---',
      '',
      '# Heading',
      '',
      '[link](https://example.com/*a*/) and `// inline code`',
      '',
      '```ts',
      '// shown',
      '```',
      '',
      '{"/* a string */"}',
    ),
    [],
  );
});

test('comments in MDX front matter are found', () => {
  assert.deepEqual(found('a.mdx', '---', '# whole line', 'title: A # trailing', '---'), [
    '2: # whole line',
    '3: # trailing',
  ]);
});

test('other files are passed over', () => {
  assert.deepEqual(found('a.json', '// not read'), []);
});

test('apostrophes in front matter do not hide a comment', () => {
  assert.deepEqual(found('a.mdx', '---', "title: Player's # one and heka's", '---'), [
    "2: # one and heka's",
  ]);
});

test('hashes inside block and long front matter values are not comments', () => {
  assert.deepEqual(
    found(
      'a.mdx',
      '---',
      'description: |',
      '  # shown',
      'title: "A',
      '  # shown too"',
      'other: "say \\"x\\" # shown"',
      '---',
    ),
    [],
  );
});

test('Markdown files are not code', () => {
  assert.deepEqual(found('a.md', '<!-- a -->', '<https://example.com> a<b {c d}'), []);
});
