import Link from 'next/link';
import type { ReactNode } from 'react';
import { Brand } from '@/components/brand';
import { ThemeToggle } from '@/components/theme-toggle';
import { releasesUrl, repository } from '@/lib/shared';
import styles from './landing.module.css';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <div className={styles.site}>
      <header className={styles.top}>
        <div className={styles.wrap}>
          <Link className={styles.brand} href="/">
            <Brand />
          </Link>
          <Link className={styles.nav} href="/docs">
            Docs
          </Link>
          <a className={styles.nav} href={repository}>
            GitHub
          </a>
          <ThemeToggle className={`${styles.nav} ${styles.square}`} />
        </div>
      </header>

      <main className={styles.main}>{children}</main>

      <footer className={styles.foot}>
        <div className={styles.wrap}>
          <nav>
            <Link href="/docs">Docs</Link>
            <a href={repository}>GitHub</a>
            <a href={releasesUrl}>All releases</a>
            <a href={`${repository}#license`}>MIT or Apache-2.0</a>
          </nav>
          <p>
            heka is not affiliated with or endorsed by Sony Interactive Entertainment or Microsoft.
            PlayStation, DualSense, DualShock and Xbox are trademarks of their owners.
          </p>
        </div>
      </footer>
    </div>
  );
}
