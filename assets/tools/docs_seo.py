#!/usr/bin/env python3
"""Add public discovery metadata to mdBook output, using only the standard library.

Run after mdbook has finished rendering. This tool does not fetch URLs, execute
page scripts, or change chapter bodies. Its output is not a readiness receipt.
"""

import argparse
import html
import json
import re
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import quote


BASE_URL = "https://decapodlabs.ai/decapod/"
SITE_URL = "https://decapodlabs.ai/"
IMAGE_URL = SITE_URL + "assets/social-preview.png"
DEFAULT_DESCRIPTION = (
    "Documentation for Decapod, the local-first governance kernel for AI coding "
    "agents: setup, workflows, boundaries, and proof-backed completion."
)
BEGIN = "<!-- decapod-public-docs:begin -->"
END = "<!-- decapod-public-docs:end -->"
OWNED_META = {
    "title", "description", "og:title", "og:description", "og:type", "og:url",
    "og:site_name", "og:image", "og:image:alt", "twitter:card", "twitter:title",
    "twitter:description", "twitter:image", "twitter:image:alt",
}
PRIVATE_ROOTS = {"admin", "signup", "auth", "api", "callback", "checkout", "billing", "account"}
HEAD = re.compile(r"(<head\b[^>]*>)(.*?)(</head\s*>)", re.I | re.S)
HEAD_TAG = re.compile(r"<(?:meta|link)\b(?:\"[^\"]*\"|'[^']*'|[^'\">])*>", re.I | re.S)
OWNED_BLOCK = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END), re.S)


def normalized(value):
    return " ".join(value.split())


def snippet(value, limit=160):
    value = normalized(value)
    if len(value) <= limit:
        return value
    return value[:limit - 1].rsplit(" ", 1)[0].rstrip(" ,;:") + "…"


def robots_directives(values):
    """Combine robots meta values without discarding existing restrictions."""
    directives = []
    seen = set()
    for value in values:
        for directive in value.split(","):
            directive = directive.strip()
            key = directive.lower()
            if directive and key not in seen:
                directives.append(directive)
                seen.add(key)
    return directives


class Page(HTMLParser):
    """Read title, metadata, and paragraph text without altering page markup."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_head = False
        self.in_title = False
        self.main_depth = 0
        self.main_seen = False
        self.skip_depth = 0
        self.heading = None
        self.paragraph = None
        self.title_parts = []
        self.first_heading = ""
        self.paragraphs = []
        self.metadata = {}
        self.robots = []
        self.redirect = False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "head":
            self.in_head = True
        if self.in_head and tag == "title":
            self.in_title = True
        if self.in_head and tag == "meta":
            key = (attrs.get("name") or attrs.get("property") or "").lower()
            self.metadata[key] = attrs.get("content") or ""
            if key == "robots":
                self.robots.append(attrs.get("content") or "")
            if (attrs.get("http-equiv") or "").lower() == "refresh":
                self.redirect = True
        if tag == "main":
            self.main_depth += 1
            self.main_seen = True
        if tag in {"script", "style", "nav", "pre", "svg"}:
            self.skip_depth += 1
        if self.main_depth and not self.skip_depth:
            if tag == "p":
                self.paragraph = []
            if tag == "h1" and not self.first_heading:
                self.heading = []
            if tag == "br" and self.paragraph is not None:
                self.paragraph.append(" ")

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False
        if tag == "head":
            self.in_head = False
        if tag == "p" and self.paragraph is not None:
            self.paragraphs.append(normalized("".join(self.paragraph)))
            self.paragraph = None
        if tag == "h1" and self.heading is not None:
            self.first_heading = normalized("".join(self.heading))
            self.heading = None
        if tag == "main":
            self.main_depth = max(0, self.main_depth - 1)
        if tag in {"script", "style", "nav", "pre", "svg"}:
            self.skip_depth = max(0, self.skip_depth - 1)

    def handle_data(self, data):
        if self.in_title:
            self.title_parts.append(data)
        if self.main_depth and not self.skip_depth:
            if self.paragraph is not None:
                self.paragraph.append(data)
            if self.heading is not None:
                self.heading.append(data)

    @property
    def title(self):
        return normalized("".join(self.title_parts))


class HeadTag(HTMLParser):
    def __init__(self, markup):
        super().__init__(convert_charrefs=True)
        self.tag = ""
        self.attrs = {}
        self.feed(markup)

    def handle_starttag(self, tag, attrs):
        self.tag = tag
        self.attrs = dict(attrs)


def replace_metadata(document, page, public, canonical, description, heading):
    head = HEAD.search(document)
    if not head:
        raise ValueError("HTML page is missing its head element")

    def retain(match):
        token = HeadTag(match.group())
        key = (token.attrs.get("name") or token.attrs.get("property") or "").lower()
        if token.tag == "meta" and (key in OWNED_META or (key == "robots" and not public)):
            return ""
        if token.tag == "link" and "canonical" in (token.attrs.get("rel") or "").lower().split():
            return ""
        return match.group()

    clean = HEAD_TAG.sub(retain, OWNED_BLOCK.sub("", head.group(2))).rstrip()
    lines = [BEGIN]
    if public:
        escape = lambda value: html.escape(value, quote=True)
        lines.append('<link rel="canonical" href="{}">'.format(escape(canonical)))
        fields = [
            ("name", "title", page.title),
            ("name", "description", description),
            ("property", "og:type", "website" if canonical == BASE_URL else "article"),
            ("property", "og:site_name", "Decapod Labs"),
            ("property", "og:title", page.title),
            ("property", "og:description", description),
            ("property", "og:url", canonical),
            ("property", "og:image", IMAGE_URL),
            ("property", "og:image:alt", "Decapod"),
            ("name", "twitter:card", "summary_large_image"),
            ("name", "twitter:title", page.title),
            ("name", "twitter:description", description),
            ("name", "twitter:image", IMAGE_URL),
            ("name", "twitter:image:alt", "Decapod"),
        ]
        lines.extend('<meta {}="{}" content="{}">'.format(attr, key, escape(value))
                     for attr, key, value in fields)
        schema = {
            "@context": "https://schema.org",
            "@type": "CollectionPage" if canonical == BASE_URL else "TechArticle",
            "@id": canonical,
            "url": canonical,
            "name": heading,
            "description": description,
            "inLanguage": "en",
            "isPartOf": {"@type": "WebSite", "url": SITE_URL, "name": "Decapod Labs"},
        }
        payload = json.dumps(schema, ensure_ascii=True, sort_keys=True).replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")
        lines.append('<script type="application/ld+json">' + payload + '</script>')
    else:
        directives = [value for value in robots_directives(page.robots)
                      if value.lower() not in {"index", "all"}]
        if not any(value.lower() == "noindex" for value in directives):
            directives.append("noindex")
        robots = html.escape(", ".join(directives), quote=True)
        lines.append('<meta name="robots" content="{}">'.format(robots))
    lines.append(END)
    replacement = clean + "\n\n" + "\n".join("        " + line for line in lines) + "\n    "
    return document[:head.start(2)] + replacement + document[head.end(2):]


def process(directory):
    root = Path(directory).resolve(strict=True)
    if not root.is_dir():
        raise ValueError("mdBook output must be a directory")
    updates = []
    pages = []
    excluded = []
    for path in sorted(root.rglob("*.html")):
        if path.is_symlink() or any(p.is_symlink() for p in path.parents if p != root):
            raise ValueError("Refusing symlinked HTML output: " + str(path))
        document = path.read_text(encoding="utf-8")
        page = Page()
        page.feed(document)
        page.close()
        relative = path.relative_to(root).as_posix()
        if not page.main_seen and not page.redirect:
            continue
        public = not (
            path.name.lower() in {"404.html", "print.html"}
            or path.relative_to(root).parts[0].lower() in PRIVATE_ROOTS
            or any(value.lower() in {"noindex", "none"}
                   for value in robots_directives(page.robots))
            or page.redirect
        )
        canonical = BASE_URL if relative == "index.html" else BASE_URL + quote(relative, safe="/")
        heading = page.first_heading or page.title
        fallback = normalized(page.metadata.get("description", "")) or DEFAULT_DESCRIPTION
        paragraph = next((p for p in page.paragraphs if len(p) >= 40), "")
        description = snippet(paragraph or fallback)
        if public and not (page.title and heading and description):
            raise ValueError("Public chapter is missing descriptive content: " + relative)
        rendered = replace_metadata(document, page, public, canonical, description, heading)
        updates.append((path, rendered))
        if public:
            pages.append((relative, canonical, heading, description))
        else:
            excluded.append(relative)
    if not any(relative == "index.html" for relative, _, _, _ in pages):
        raise ValueError("No public mdBook index.html was found; nothing was written")
    pages.sort(key=lambda page: (page[0] != "index.html", page[0]))
    sitemap = '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    sitemap += "".join("  <url><loc>" + html.escape(url) + "</loc></url>\n" for _, url, _, _ in pages)
    sitemap += "</urlset>\n"
    index = "# Decapod documentation\n\n> " + DEFAULT_DESCRIPTION + "\n\n## Documentation\n\n"
    for _, url, heading, description in pages:
        label = heading.replace("\\", "\\\\").replace("[", "\\[").replace("]", "\\]")
        index += "- [{}]({}): {}\n".format(label, url, description)
    for name, content in (("sitemap.xml", sitemap), ("llms.txt", index)):
        path = root / name
        if path.is_symlink():
            raise ValueError("Refusing symlinked discovery output: " + str(path))
        updates.append((path, content))
    for path, content in updates:
        if not path.exists() or path.read_text(encoding="utf-8") != content:
            path.write_text(content, encoding="utf-8")
    return {"public_pages": len(pages), "excluded_pages": excluded, "canonical_base": BASE_URL}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", help="Completed mdBook HTML output directory")
    args = parser.parse_args()
    try:
        report = process(args.directory)
    except (OSError, ValueError) as error:
        parser.exit(1, "docs-seo: {}\n".format(error))
    print("docs-seo: " + json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
