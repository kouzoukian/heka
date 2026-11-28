import assert from 'node:assert/strict';
import { test } from 'node:test';
import { downloadIn, latestDownload } from '../lib/release.ts';

const release = {
  tag_name: 'v0.2.0',
  assets: [{ name: 'heka-v0.2.0-windows-x64.zip', size: 10783721 }],
};

test('the zip of a release becomes the download, with its version and size', () => {
  assert.deepEqual(downloadIn(release), {
    version: 'v0.2.0',
    size: '10.3 MB',
    url: 'https://github.com/kouzoukian/heka/releases/download/v0.2.0/heka-v0.2.0-windows-x64.zip',
  });
});

test('other assets are passed over', () => {
  const assets = [{ name: 'heka-v0.2.0-linux-x64.zip', size: 1 }, ...release.assets];

  assert.equal(downloadIn({ ...release, assets })?.size, '10.3 MB');
});

for (const [what, response] of [
  ['a rate limit answer', { message: 'API rate limit exceeded' }],
  ['nothing', null],
  ['text', '<html>'],
  ['a release without the zip', { tag_name: 'v0.2.0', assets: [] }],
  ['assets that are not objects', { tag_name: 'v0.2.0', assets: [null] }],
  ['a tag that is not a version', { tag_name: '../../evil', assets: [{ name: 'heka-../../evil-windows-x64.zip', size: 1 }] }],
  ['a zip without a size', { tag_name: 'v0.2.0', assets: [{ name: 'heka-v0.2.0-windows-x64.zip' }] }],
] as const) {
  test(`${what} gives no download`, () => {
    assert.equal(downloadIn(response), null);
  });
}

const answering = (status: number, body: string) => async () => new Response(body, { status });

test('the latest release is asked of GitHub', async () => {
  const asked: string[] = [];
  const download = await latestDownload(async (url) => {
    asked.push(String(url));
    return new Response(JSON.stringify(release));
  });

  assert.deepEqual(asked, ['https://api.github.com/repos/kouzoukian/heka/releases/latest']);
  assert.equal(download?.version, 'v0.2.0');
});

for (const [what, ask] of [
  ['a refusal', answering(403, JSON.stringify(release))],
  ['an answer that is not JSON', answering(200, '<html>')],
  [
    'no network',
    async () => {
      throw new TypeError('Failed to fetch');
    },
  ],
] as const) {
  test(`${what} leaves no download`, async () => {
    assert.equal(await latestDownload(ask), null);
  });
}
