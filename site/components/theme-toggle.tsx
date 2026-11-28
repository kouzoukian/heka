'use client';

import { useTheme } from 'fumadocs-ui/provider/base';
import { Contrast } from 'lucide-react';

export function ThemeToggle({ className }: { className?: string }) {
  const { resolvedTheme, setTheme } = useTheme();

  return (
    <button
      type="button"
      className={className}
      aria-label="Switch theme"
      onClick={() => setTheme(resolvedTheme === 'light' ? 'dark' : 'light')}
    >
      <Contrast />
    </button>
  );
}
