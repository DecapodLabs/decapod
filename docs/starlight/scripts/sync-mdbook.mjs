import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const source = path.resolve(here, '../../../docs/book/src');
const destination = path.resolve(here, '../src/content/docs');
const toPosix = (p) => p.split(path.sep).join('/');
const errors = [];

function collect(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const absolute = path.join(dir, entry.name);
    return entry.isDirectory() ? collect(absolute) : [absolute];
  });
}

const contentFiles = collect(source)
  .filter(p => p.endsWith('.md') && path.basename(p) !== 'SUMMARY.md');
const sourcePages = new Set(contentFiles.map(p => toPosix(path.relative(source, p))));
const targetSlug = (relative) => {
  const withoutExtension = relative.replace(/\.md$/, '');
  return withoutExtension === 'index' ? '' : withoutExtension;
};

function resolveLink(from, original) {
  if (/^(?:[a-z][a-z\d+.-]*:|\/|#)/i.test(original)) return original;
  const [pathname, fragment = ''] = original.split('#', 2);
  const file = pathname.split('?')[0];
  if (!/\.(?:md|html)$/.test(file)) return original;
  const relative = toPosix(path.posix.normalize(path.posix.join(
    path.posix.dirname(from), file.replace(/\.html$/, '.md')
  )));
  if (!sourcePages.has(relative)) {
    errors.push(from + ': unresolved documentation link ' + original);
    return original;
  }
  return '/decapod/' + (targetSlug(relative) ? targetSlug(relative) + '/' : '') +
    (fragment ? '#' + fragment : '');
}

fs.rmSync(destination, { recursive: true, force: true });
fs.mkdirSync(destination, { recursive: true });

for (const absolute of contentFiles) {
  const relative = toPosix(path.relative(source, absolute));
  const contents = fs.readFileSync(absolute, 'utf8');
  const heading = contents.match(/^#\s+(.+)$/m);
  const title = heading ? heading[1].replace(/[*_`]/g, '') :
    path.basename(relative, '.md').replace(/-/g, ' ');
  const converted = contents
    .replace(/(\[[^\]]*\]\()([^\s)]+)(\))/g,
      (whole, open, href, close) => open + resolveLink(relative, href) + close)
    .replace(/(href=\")([^\"]+)(\")/g,
      (whole, open, href, close) => open + resolveLink(relative, href) + close);
  const header = '---\ntitle: ' + JSON.stringify(title) + '\n' +
    (relative === 'index.md' ? 'template: splash\n' : '') + '---\n\n';
  const target = path.join(destination, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, header + converted);
}
if (errors.length) {
  for (const e of errors) console.error(e);
  process.exitCode = 1;
}
console.log('Prepared ' + contentFiles.length + ' Markdown pages from canonical mdBook source.');
