import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://decapodlabs.github.io',
  base: '/decapod',
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'Decapod — Documentation',
      description: 'Repo-native governance for bounded, verifiable agent work.',
      defaultLocale: 'root',
      favicon: '/decapod/favicon.svg',
      editLink: { baseUrl: 'https://github.com/DecapodLabs/decapod/edit/master/docs/book/src/' },
      head: [{ tag: 'meta', attrs: { name: 'theme-color', content: '#fff9f0' } }],
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/DecapodLabs/decapod' }],
      components: {
        SiteTitle: './src/components/SiteTitle.astro',
        Footer: './src/components/Footer.astro',
      },
      customCss: ['./src/styles/plush.css'],
      sidebar: [
        { slug: 'introduction' },
        { slug: 'quickstart' },
        { slug: 'mental-model' },
        { slug: 'configuration' },
        { label: 'Workflows', autogenerate: { directory: 'workflows' } },
        { label: 'Concepts', autogenerate: { directory: 'concepts' } },
        { label: 'Reference', autogenerate: { directory: 'reference' } },
        { slug: 'contributing' },
      ],
    }),
  ],
});
