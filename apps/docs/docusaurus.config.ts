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
        href: 'https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&family=Geist+Mono:wght@400;500;600&display=swap',
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
      hideOnScroll: false,
      items: [
        {type: 'docSidebar', sidebarId: 'start', position: 'left', label: 'Docs'},
        {type: 'docSidebar', sidebarId: 'editor', position: 'left', label: 'Editor'},
        {type: 'docSidebar', sidebarId: 'scripting', position: 'left', label: 'Scripting'},
        {type: 'docSidebar', sidebarId: 'runtimes', position: 'left', label: 'Runtimes'},
        {type: 'docSidebar', sidebarId: 'usecases', position: 'left', label: 'Use cases'},
        {type: 'docSidebar', sidebarId: 'games', position: 'left', label: 'Games'},
        {type: 'docSidebar', sidebarId: 'courses', position: 'left', label: 'Courses'},
        {href: 'https://toastycreate.web.app/', label: 'Open the editor', position: 'right', target: '_self', className: 'navbar-cta'},
      ],
    },
    footer: {
      style: 'light',
      links: [
        {
          title: 'Learn',
          items: [
            {label: 'Get started', to: '/getting-started/first-project'},
            {label: 'Courses', to: '/courses'},
            {label: 'Scripting cheatsheet', to: '/scripting/cheatsheet'},
          ],
        },
        {
          title: 'Ship it',
          items: [
            {label: 'Toasty Lite runtime', to: '/runtimes/lite'},
            {label: 'Use cases', to: '/use-cases'},
            {label: 'Games', to: '/games'},
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
      theme: prismThemes.github,
      darkTheme: prismThemes.vsDark,
      additionalLanguages: ['bash', 'json', 'rust'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
