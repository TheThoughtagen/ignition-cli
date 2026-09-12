import {readFile, writeFile} from 'node:fs/promises';

const source = await readFile(new URL('../README.md', import.meta.url), 'utf8');
// Keep the README's command contracts authoritative. Relative repository links
// become source links because this page lives under /docs/ on GitHub Pages.
const linked = source.replace(/\]\(([^\s)]+)\)/g, (match, target) => {
  if (/^(?:[a-z]+:|#|\/\/)/i.test(target)) return match;
  return `](https://github.com/TheThoughtagen/ignition-cli/blob/main/${target.replace(/^\.\//, '')})`;
});
await writeFile(new URL('../docs/reference.md', import.meta.url), linked.replace(/^# .+\n/, '# Command reference\n'));
