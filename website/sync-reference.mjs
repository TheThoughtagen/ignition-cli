import {readFile, writeFile, mkdir} from 'node:fs/promises';
import { existsSync } from 'node:fs';
import GithubSlugger from 'github-slugger';

const REPO = 'https://github.com/TheThoughtagen/ignition-cli/blob/main/';
const README = new URL('../README.md', import.meta.url);
const OUT = new URL('../docs/reference/', import.meta.url);
const STATIC = new URL('static/', import.meta.url);

const source = await readFile(README, 'utf8');
const lines = source.split('\n');

// ---------------------------------------------------------------------------
// Pass 1: collect every heading in document order and compute its anchor with
// the same slugger docusaurus's rehype-slug uses, so intra-doc anchor links
// can be rewritten to (page, anchor) pairs that actually resolve.
// ---------------------------------------------------------------------------
const slugger = new GithubSlugger();
const anchorPage = new Map(); // anchor -> page slug
const headingsByPage = new Map(); // page slug -> [{level, text, anchor}]

// Split into sections on top-level `## ` boundaries.
const sections = [];
let current = {title: 'Overview', slug: 'index', lines: []};
let inSection = false;
for (const line of lines) {
  const m = line.match(/^## (.+)$/) ?? (line.match(/^# (.+)$/) && inSection === false ? null : null);
  if (/^## /.test(line)) {
    if (current.lines.length > 0 || current.slug !== 'index') sections.push(current);
    const title = line.replace(/^## /, '').trim();
    current = {title, slug: slugify(title), lines: []};
    inSection = true;
  } else {
    current.lines.push(line);
  }
}
if (current.lines.length > 0) sections.push(current);

function slugify(text) {
  // filename slug: strip markdown, lowercase, keep [a-z0-9-]
  const t = text.replace(/[`*_~]/g, '').replace(/[^\w\s-]/g, '').trim().replace(/\s+/g, '-').toLowerCase();
  return t || 'section';
}

// ---------------------------------------------------------------------------
// Pass 2: assign anchors per heading (strip markdown chars the way rehype
// sees rendered text), mapping each anchor to its owning page.
// ---------------------------------------------------------------------------
const anchorSlugger = new GithubSlugger();
const titleAnchor = new Map(); // page slug -> the anchor of its own section title
for (const section of sections) {
  const headings = [];
  // The section title itself is an h2 on its page; rehype sees it as an h2 too.
  headings.push({level: 2, text: section.title, anchor: anchorSlugger.slug(stripMd(section.title))});
  titleAnchor.set(section.slug, headings[0].anchor);
  for (const line of section.lines) {
    const m = line.match(/^(#{2,4}) (.+)$/);
    if (m) {
      const text = m[2].trim();
      headings.push({level: m[1].length, text, anchor: anchorSlugger.slug(stripMd(text))});
    }
  }
  headingsByPage.set(section.slug, headings);
  for (const h of headings) anchorPage.set(h.anchor, section.slug);
}

function stripMd(text) {
  return text.replace(/[`*_~[\]]/g, '');
}

// ---------------------------------------------------------------------------
// Pass 3: rewrite links. Relative repo paths -> GitHub blob URLs; intra-doc
// #anchors -> correct subpage anchors.
// ---------------------------------------------------------------------------
function rewriteLinks(body, ownPage) {
  return body.replace(/\]\(([^\s)]+)\)/g, (match, target) => {
    if (/^(?:[a-z]+:|\/\/)/i.test(target)) return match; // absolute URL
    if (target.startsWith('#')) {
      const anchor = target.slice(1);
      const page = anchorPage.get(anchor);
      if (!page) return match; // unknown anchor: leave untouched
      if (anchor === titleAnchor.get(page)) {
        // the section-title anchor: link the page itself (docusaurus swallows
        // the body H1 that duplicates the frontmatter title, so a fragment
        // for it would never resolve)
        if (page === ownPage) return match; // self title: leave as-is (does not occur in the README today)
        return `](../${page === 'index' ? '' : page}/)`;
      }
      if (page === ownPage) return `](#${anchor})`;
      return `](../${page === 'index' ? '' : page}#${anchor})`;
    }
    if (target.includes('#')) {
      // path#anchor: not used in the README today; leave untouched
      return match;
    }
    // plain repo-relative path -> GitHub blob link
    return `](${REPO}${target.replace(/^\.\//, '')})`;
  });
}

// ---------------------------------------------------------------------------
// Pass 4: emit pages.
// ---------------------------------------------------------------------------
if (existsSync(OUT)) {
  const {rm} = await import('node:fs/promises');
  await rm(OUT, {recursive: true});
}
await mkdir(OUT, {recursive: true});

const searchEntries = [];
let position = 1;

for (const section of sections) {
  const body = section.lines.join('\n').replace(/^\n+/, '').trimEnd();
  const rewritten = rewriteLinks(body, section.slug);
  const isIndex = section.slug === 'index';
  const fileName = isIndex ? 'index.md' : `${section.slug}.md`;
  // The section's own `## title` line was consumed by the split — re-emit it
  // as the page H1 so heading anchors pointing at the section title resolve.
  const frontmatter = [
    '---',
    `title: ${JSON.stringify(section.title)}`,
    `sidebar_position: ${isIndex ? 0 : position}`,
    '---',
    '',
  ].join('\n');

  let pageBody = `${isIndex ? '' : `# ${section.title}\n\n`}${rewritten}`;
  if (isIndex) {
    const toc = sections
      .filter((s) => s.slug !== 'index')
      .map((s) => `| [${s.title}](./${s.slug}.md) |`)
      .join('\n');
    pageBody = `${body}\n\n## In this reference\n\n| Section |\n|---------|\n${toc}\n`;
  }

  await writeFile(new URL(fileName, OUT), `${frontmatter}\n${pageBody}\n`);

  // Search index: page entry + one entry per heading with the text that follows it.
  const urlBase = `/ignition-cli/docs/reference/${isIndex ? '' : section.slug}`;
  searchEntries.push({url: urlBase, title: section.title, heading: '', anchor: '', text: textOf(body.slice(0, 800))});
  for (const h of headingsByPage.get(section.slug) ?? []) {
    if (h.level === 2 && h.text === section.title) continue;
    const idx = body.indexOf(h.text);
    const excerpt = idx >= 0 ? textOf(body.slice(idx + h.text.length, idx + h.text.length + 400)) : '';
    searchEntries.push({
      url: `${urlBase}#${h.anchor}`,
      title: section.title,
      heading: h.text,
      anchor: h.anchor,
      text: excerpt,
    });
  }
  position += 1;
}

await writeFile(new URL('_category_.json', OUT), JSON.stringify({label: 'Reference', position: 3}, null, 2) + '\n');

// ---------------------------------------------------------------------------
// Search index for the cmd+k palette.
// ---------------------------------------------------------------------------
await mkdir(STATIC, {recursive: true});
await writeFile(new URL('search-index.json', STATIC), JSON.stringify(searchEntries));

console.log(`sync-reference: ${sections.length} pages, ${searchEntries.length} search entries`);

function textOf(md) {
  return md
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/\|/g, ' ')
    .replace(/[#>*`_~[\]()]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 300);
}
