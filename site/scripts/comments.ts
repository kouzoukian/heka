import { createRequire } from 'node:module';
import postcss from 'postcss';
import remarkFrontmatter from 'remark-frontmatter';
import remarkMdx from 'remark-mdx';
import remarkParse from 'remark-parse';
import ts from 'typescript';
import { unified } from 'unified';
import { Parser } from 'yaml';

export type Comment = {
  line: number;
  text: string;
};

type CssTokens = {
  endOfFile(): boolean;
  nextToken(): [type: string, text: string, start: number];
};

const tokenizeCss: (input: InstanceType<typeof postcss.Input>) => CssTokens = createRequire(
  import.meta.url,
)('postcss/lib/tokenize');

function lineAt(source: string, offset: number) {
  return source.slice(0, offset).split('\n').length;
}

function inScript(path: string, source: string): Comment[] {
  const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  const ranges = new Map<number, number>();
  const jsxTexts: ts.Node[] = [];
  const visit = (node: ts.Node) => {
    if (node.kind >= ts.SyntaxKind.FirstJSDocNode && node.kind <= ts.SyntaxKind.LastJSDocNode) return;
    if (ts.isJsxText(node)) {
      jsxTexts.push(node);
      return;
    }
    const children = node.getChildren(file);
    if (children.length === 0) {
      for (const range of [
        ...(ts.getLeadingCommentRanges(source, node.pos) ?? []),
        ...(ts.getTrailingCommentRanges(source, node.end) ?? []),
      ]) {
        ranges.set(range.pos, range.end);
      }
    }
    children.forEach(visit);
  };
  visit(file);
  return [...ranges]
    .filter(([start]) => !jsxTexts.some((text) => start >= text.pos && start < text.end))
    .sort(([a], [b]) => a - b)
    .map(([start, end]) => ({
      line: lineAt(source, start),
      text: source.slice(start, end),
    }));
}

function inCss(source: string): Comment[] {
  const tokens = tokenizeCss(new postcss.Input(source));
  const comments: Comment[] = [];
  while (!tokens.endOfFile()) {
    const [type, text, start] = tokens.nextToken();
    if (type === 'comment') comments.push({ line: lineAt(source, start), text });
  }
  return comments;
}

function inFrontMatter(yaml: string, firstLine: number): Comment[] {
  const comments: Comment[] = [];
  const visit = (token: unknown) => {
    if (typeof token !== 'object' || token === null) return;
    if ('type' in token && token.type === 'comment' && 'source' in token && 'offset' in token) {
      const line = firstLine + lineAt(yaml, Number(token.offset)) - 1;
      comments.push({ line, text: String(token.source).trimEnd() });
    }
    Object.values(token).forEach(visit);
  };
  visit([...new Parser().parse(yaml)]);
  return comments;
}

const mdxParser = unified().use(remarkParse).use(remarkFrontmatter).use(remarkMdx);

type EstreeComment = {
  type: 'Line' | 'Block';
  value: string;
  loc: { start: { line: number } };
};

function inMdx(source: string): Comment[] {
  const comments: Comment[] = [];
  const visit = (node: unknown) => {
    if (typeof node !== 'object' || node === null) return;
    if ('type' in node && node.type === 'yaml' && 'value' in node) {
      comments.push(...inFrontMatter(String(node.value), 2));
    }
    if ('type' in node && node.type === 'Program' && 'comments' in node) {
      for (const { type, value, loc } of node.comments as EstreeComment[]) {
        const text = type === 'Line' ? `//${value}` : `/*${value}*/`;
        comments.push({ line: loc.start.line, text });
      }
    }
    Object.values(node).forEach(visit);
  };
  visit(mdxParser.parse(source));
  return comments;
}

export function commentsIn(path: string, source: string): Comment[] {
  if (/\.[cm]?[jt]sx?$/.test(path)) return inScript(path, source);
  if (path.endsWith('.css')) return inCss(source);
  if (path.endsWith('.mdx')) return inMdx(source);
  return [];
}
