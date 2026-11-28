'use client';

import { Download as DownloadIcon } from 'lucide-react';
import { useEffect, useState } from 'react';
import { type Download, latestDownload } from '@/lib/release';
import { latestReleaseUrl } from '@/lib/shared';

type DownloadButtonProps = {
  className?: string;
  detailClassName?: string;
};

export function DownloadButton({ className, detailClassName }: DownloadButtonProps) {
  const [download, setDownload] = useState<Download | null>();

  useEffect(() => {
    latestDownload().then(setDownload);
  }, []);

  return (
    <a className={className} href={download?.url ?? latestReleaseUrl}>
      <DownloadIcon />
      <span>{download === null ? 'Download from GitHub Releases' : 'Download for Windows'}</span>
      {download && (
        <span className={detailClassName}>
          {download.version} · {download.size}
        </span>
      )}
    </a>
  );
}
