import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// ToastyCreate docs. Served from the editor's own Firebase site under
// /docs/ (apps/editor's deploy copies this build into dist/docs).
const config: Config = {
  title: 'Toasty Docs',
  tagline: 'Animate it, script it, ship it to the web.',
  favicon: 'img/favicon.svg',
  future: {v4: true},
  url: 'https://toastycreate.web.app',
  baseUrl: '/docs/',
  trailingSlash: false,
  onBrokenLinks: 'warn',
  markdown: {hooks: {onBrokenMarkdownLinks: 'warn'}},
  i18n: {defaultLocale: 'en', locales: ['en']},
  headTags: [
    {tagName: 'link', attributes: {rel: 'preconnect', href: 'https://fonts.googleapis.com'}},
    {tagName: 'link', attributes: {rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: 'anonymous'}},
    {
      tagName: 'link',
      attributes: {
        rel: 'stylesheet',
        href: 'https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,500;12..96,700;12..96,800&family=Figtree:wght@400;500;600;700&family=JetBrains+Mono:wght@400;600&display=swap',
      },
    },
  ],
  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
        },
        blog: false,
        theme: {customCss: './src/css/custom.css'},
      } satisfies Preset.Options,
    ],
  ],
  themes: ['@docusaurus/theme-live-codeblock'],
  themeConfig: {
    colorMode: {respectPrefersColorScheme: true},
    liveCodeBlock: {playgroundPosition: 'top'},
    navbar: {
      title: 'Toasty',
      logo: {alt: 'Toasty', src: 'img/favicon.svg'},
      items: [
        {type: 'doc', docId: 'intro', position: 'left', label: 'Start'},
        {type: 'docSidebar', sidebarId: 'editor', position: 'left', label: 'Editor'},
        {type: 'docSidebar', sidebarId: 'components', position: 'left', label: 'Components'},
        {type: 'docSidebar', sidebarId: 'scripting', position: 'left', label: 'Scripting'},
        {type: 'docSidebar', sidebarId: 'embedding', position: 'left', label: 'Embedding'},
        {type: 'docSidebar', sidebarId: 'courses', position: 'left', label: 'Courses'},
        {to: '/showcase', label: 'Showcase', position: 'left'},
        {href: 'https://toastycreate.web.app/', label: 'Open the editor', position: 'right', target: '_self', className: 'navbar-cta'},
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Learn',
          items: [
            {label: 'Your first project', to: '/getting-started/first-project'},
            {label: 'Courses', to: '/courses'},
            {label: 'Scripting cheatsheet', to: '/scripting/cheatsheet'},
          ],
        },
        {
          title: 'Build for the web',
          items: [
            {label: 'React embeds', to: '/embedding/react'},
            {label: 'API reference', to: '/embedding/api-reference'},
            {label: 'Showcase', to: '/showcase'},
          ],
        },
        {
          title: 'Toasty',
          items: [{label: 'Open the editor', href: 'https://toastycreate.web.app/', target: '_self'}],
        },
      ],
      copyright: `ToastyCreate · ${new Date().getFullYear()}`,
    },
    prism: {
      theme: prismThemes.oneLight,
      darkTheme: prismThemes.oneDark,
      additionalLanguages: ['bash', 'json', 'rust'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
