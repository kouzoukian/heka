'use client';

import { useTheme } from 'fumadocs-ui/provider/base';
import { Moon, Sun } from 'lucide-react';

export function ThemeToggle({ className }: { className?: string }) {
  const { resolvedTheme, setTheme } = useTheme();

  return (
    <button
      type="button"
      className={className}
      aria-label="Switch theme"
      onClick={() => setTheme(resolvedTheme === 'light' ? 'dark' : 'light')}
    >
      <Sun className="hidden dark:block" />
      <Moon className="dark:hidden" />
    </button>
  );
}
