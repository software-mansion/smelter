import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import tailwindPlugin from './plugins/tailwind-config.cjs';
import copyTypeFilesPlugin from './plugins/live-compositor-types.js';

const config: Config = {
  title: 'LiveCompositor',
  favicon: 'img/favicon.png',

  // Set the production url of your site here
  url: 'https://compositor.live',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'software-mansion', // Usually your GitHub org/user name.
  projectName: 'live-compositor', // Usually your repo name.

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'throw',
  onBrokenAnchors: 'throw',
  trailingSlash: false,

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          path: 'pages',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
        gtag: {
          trackingID: 'G-SEF91C2QGB',
          anonymizeIP: true,
        },
      } satisfies Preset.Options,
    ],
  ],

  plugins: [
    [
      '@docusaurus/plugin-client-redirects',
      {
        redirects: [
          {
            to: '/docs/intro',
            from: '/docs',
          },
        ],
      },
    ],
    tailwindPlugin,
    copyTypeFilesPlugin,
  ],

  themeConfig: {
    metadata: [
      { name: 'description', content: 'Real-time, low latency, programmable video & audio mixer' },
      { name: 'keywords', content: 'video, audio, mixing, real-time, live' },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:title', content: 'LiveCompositor' },
      {
        name: 'twitter:description',
        content: 'Real-time, low latency, programmable video & audio mixer',
      },
      { name: 'twitter:site', content: 'ElixirMembrane' },
      { name: 'og:type', content: 'website' },
      { name: 'og:image', content: 'https://compositor.live/img/logo.webp' },
      { name: 'og:title', content: 'LiveCompositor' },
      {
        name: 'og:description',
        content: 'Real-time, low latency, programmable video & audio mixer',
      },
      { name: 'og:url', content: 'https://compositor.live/' },
    ],
    colorMode: {
      defaultMode: 'dark',
    },
    navbar: {
      logo: {
        alt: 'logo',
        src: 'img/logo.svg',
        srcDark: 'img/logo-dark.svg',
      },
      items: [
        {
          to: '/docs/intro',
          position: 'right',
          className: 'navbar-docs-link',
          'aria-label': 'Docs',
        },
        {
          to: 'https://github.com/software-mansion/live-compositor',
          position: 'right',
          className: 'navbar-github-link',
          'aria-label': 'GitHub repository',
        },
      ],
    },
    announcementBar: {
      id: 'smelter-redirect',
      content:
        '<span id="smelter-redirect-content"><strong>Live Compositor is now Smelter. <span style="color: #F24664;"><a target="_blank" rel="noopener noreferrer" style="text-decoration: none;" href="https://smelter.dev">Visit smelter.dev</a></span> to see the most recent version.</span></strong>',
      backgroundColor: '#161127',
      textColor: '#FFFFFF',
      isCloseable: false,
    },
    footer: {
      style: 'dark',
      copyright: `Copyright © ${new Date().getFullYear()} Software Mansion S.A.`,
    },
    prism: {
      theme: prismThemes.oneDark,
      additionalLanguages: ['wgsl', 'http', 'elixir', 'bash'],
    },
    algolia: {
      appId: 'AB30AX8OU1',
      apiKey: '4dae5f71952b8ebd63dd7645128c3b24',
      indexName: 'compositor',
      contextualSearch: false,
    },
  } satisfies Preset.ThemeConfig,
  customFields: {
    environment: process.env.ENVIRONMENT,
  },
};

export default config;
