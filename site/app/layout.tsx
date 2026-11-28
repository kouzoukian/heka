import type { Metadata } from 'next';
import localFont from 'next/font/local';
import { Provider } from '@/components/provider';
import { appName, siteUrl, tagline } from '@/lib/shared';
import './global.css';

const code = localFont({
  src: './fonts/GoogleSansCode-VariableFont_wght.ttf',
  weight: '300 800',
  variable: '--font-code',
});

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: { default: appName, template: `%s | ${appName}` },
  description: tagline,
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={code.variable} suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
