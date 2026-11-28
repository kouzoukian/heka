import { latestReleaseApi, releasesUrl } from './shared.ts';

export type Download = {
  version: string;
  size: string;
  url: string;
};

type Asset = {
  name: string;
  size: number;
};

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isAsset(value: unknown): value is Asset {
  return isObject(value) && typeof value.name === 'string' && typeof value.size === 'number';
}

export function downloadIn(response: unknown): Download | null {
  if (!isObject(response) || !Array.isArray(response.assets)) return null;
  const version = response.tag_name;
  if (typeof version !== 'string' || !/^v\d+\.\d+\.\d+$/.test(version)) return null;
  const name = `heka-${version}-windows-x64.zip`;
  const zip = response.assets.filter(isAsset).find((asset) => asset.name === name);
  if (!zip) return null;
  return {
    version,
    size: `${(zip.size / 1024 / 1024).toFixed(1)} MB`,
    url: `${releasesUrl}/download/${version}/${name}`,
  };
}

export async function latestDownload(ask: typeof fetch = fetch): Promise<Download | null> {
  try {
    const answer = await ask(latestReleaseApi);
    return answer.ok ? downloadIn(await answer.json()) : null;
  } catch {
    return null;
  }
}
