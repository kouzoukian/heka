import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { Brand } from '@/components/brand';
import { repository } from '@/lib/shared';
import { source } from '@/lib/source';

export default function Layout({ children }: LayoutProps<'/docs'>) {
  return (
    <DocsLayout tree={source.getPageTree()} nav={{ title: <Brand /> }} githubUrl={repository}>
      {children}
    </DocsLayout>
  );
}
