import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const source = path.resolve(here, '../../../docs/book/src');
const dest = path.resolve(here, '../src/content/docs');
const inventory = path.resolve(here, '../content-inventory.json');
const toSlash = p => p.split(path.sep).join('/');
const list = dir => fs.readdirSync(dir, {withFileTypes: true}).flatMap(e => {
  const file = path.join(dir,e.name);
  return e.isDirectory() ? list(file) : [file];
});
const originals = list(source).filter(p=>p.endsWith('.md') && path.basename(p)!=='SUMMARY.md');
const pages = new Set(originals.map(p=>toSlash(path.relative(source,p))));
const slug = relative => relative.replace(/\.md$/,'').replace(/^index$/,'');
const errors=[];

function href(from,value) {
  if (/^(?:[a-z][\w+.-]*:|\/|#)/i.test(value)) return value;
  const match = value.match(/^([^?#]*)(\?[^#]*)?(#.*)?$/);
  if (!match) return value;
  const [,pathname,query='',hash=''] = match;
  if (!/\.(?:html|md)$/i.test(pathname)) return value;
  const resolved = toSlash(path.posix.normalize(path.posix.join(path.posix.dirname(from),pathname.replace(/\.html$/i,'.md'))));
  if (!pages.has(resolved)) {
    // Non-book relative references are unchanged, but visible for auditing.
    if(!resolved.startsWith('../')) errors.push(from+' -> '+value);
    return value;
  }
  return '/decapod/'+(slug(resolved) ? slug(resolved)+'/' : '')+query+hash;
}

fs.rmSync(dest,{recursive:true,force:true});
fs.mkdirSync(dest,{recursive:true});
const manifest=[];
for(const file of originals) {
  const relative = toSlash(path.relative(source,file));
  let markdown=fs.readFileSync(file,'utf8');
  const title=(markdown.match(/^#\s+(.+)$/m)||[])[1]?.replace(/[*_`]/g,'')||path.basename(relative,'.md').replace(/-/g,' ');
  markdown=markdown.replace(/(\[[^\]]*\]\()([^\s)]+)(\))/g,(_,a,url,b)=>a+href(relative,url)+b);
  markdown=markdown.replace(/(href=")([^"]+)(")/g,(_,a,url,b)=>a+href(relative,url)+b);
  const header='---\ntitle: '+JSON.stringify(title)+'\n'+(relative==='index.md'?'template: splash\n':'')+'---\n\n';
  const target=path.join(dest,relative);
  fs.mkdirSync(path.dirname(target),{recursive:true});
  fs.writeFileSync(target,header+markdown);
  manifest.push({source:relative,slug:slug(relative),title});
}
fs.writeFileSync(inventory,JSON.stringify({pages:manifest},null,2)+'\n');
if(errors.length) throw Error('Missing target documentation pages:\n'+errors.join('\n'));
console.log('Synced '+manifest.length+' canonical Markdown documentation pages.');
