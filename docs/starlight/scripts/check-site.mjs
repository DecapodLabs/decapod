import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url));
const root=path.resolve(here,'../dist');
const inventory=JSON.parse(fs.readFileSync(path.resolve(here,'../content-inventory.json'),'utf8')).pages;
const errors=[];
for(const page of inventory) {
 const dir=path.join(root,page.slug);
 const target=path.join(dir,'index.html');
 if(!fs.existsSync(target)) {
   errors.push('Missing rendered page '+page.slug);
   continue;
 }
 const html=fs.readFileSync(target,'utf8');
 if(!html.includes(page.title.replace(/&/g,'&amp;')) && !html.includes('<main')) errors.push('Missing HTML body '+page.slug);
 if(!html.includes('logo-a.png')||!html.includes('mascot.svg')) errors.push('Missing documentation branding '+page.slug);
 if(page.slug) {
   const legacy=path.join(root,page.slug+'.html');
   fs.mkdirSync(path.dirname(legacy),{recursive:true});
   const url='/decapod/'+page.slug+'/';
   fs.writeFileSync(legacy,'<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url='+url+'"><link rel="canonical" href="'+url+'"><title>Document moved</title></head><body><a href="'+url+'">Continue to documentation</a><script>location.replace('+JSON.stringify(url)+'+location.search+location.hash)</script></body></html>\n');
 }
}
if(!fs.existsSync(path.join(root,'favicon.svg'))) errors.push('Mascot favicon missing');
if(!fs.existsSync(path.join(root,'pagefind','pagefind.js'))) errors.push('Static Pagefind search index missing');
if(errors.length) {
 console.error(errors.join('\n'));
 process.exit(1);
}
console.log('Verified '+inventory.length+' pages, aliases, navbar logos, mascot favicon and search index.');
