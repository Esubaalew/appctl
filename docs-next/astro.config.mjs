// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import remarkPrefixBase from './remark-prefix-base.mjs';
import remarkAppctlVersion from './remark-appctl-version.mjs';
import { APPCTL_VERSION } from './src/lib/version.mjs';

// GitHub Pages serves this project site under /appctl/.
// Set APPCTL_DOCS_BASE=/ to build a root-served preview locally.
const base = process.env.APPCTL_DOCS_BASE ?? '/appctl/';
const site = process.env.APPCTL_DOCS_SITE ?? 'https://esubaalew.github.io';

const ogUrl = new URL(`${base.replace(/\/$/, '')}/og.png`, site).toString();

export default defineConfig({
  site,
  base,
  trailingSlash: 'always',
  markdown: {
    remarkPlugins: [
      [remarkPrefixBase, { base }],
      [remarkAppctlVersion, { version: APPCTL_VERSION }],
    ],
  },
  redirects: (() => {
    const b = base.endsWith('/') ? base : `${base}/`;
    const bp = (p) => `${b.replace(/\/$/, '')}${p}`;
    return {
      '/docs/': bp('/docs/introduction/'),
      '/sources/openapi/': bp('/docs/sources/openapi/'),
      '/sources/django/': bp('/docs/sources/django/'),
      '/sources/rails/': bp('/docs/sources/rails/'),
      '/sources/laravel/': bp('/docs/sources/laravel/'),
      '/sources/aspnet/': bp('/docs/sources/aspnet/'),
      '/sources/strapi/': bp('/docs/sources/strapi/'),
      '/sources/supabase/': bp('/docs/sources/supabase/'),
      '/sources/db/': bp('/docs/sources/db/'),
      '/sources/url/': bp('/docs/sources/url/'),
      '/sources/mcp/': bp('/docs/sources/mcp/'),
      '/sources/plugins/': bp('/docs/sources/plugins/'),
      '/sources/choosing-a-sync-source/': bp('/docs/sources/choosing-a-sync-source/'),
      '/deploy/': bp('/docs/deploy/local/'),
    };
  })(),
  integrations: [
    starlight({
      title: 'appctl',
      description:
        'Talk to your app in plain English. appctl introspects your app, generates tools, and runs an auditable agent loop.',
      logo: {
        src: './src/assets/logo.svg',
        replacesTitle: true,
      },
      favicon: '/favicon.svg',
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/Esubaalew/appctl',
        },
      ],
      customCss: [
        '@fontsource/inter/400.css',
        '@fontsource/inter/500.css',
        '@fontsource/inter/600.css',
        '@fontsource/inter/700.css',
        '@fontsource/jetbrains-mono/400.css',
        '@fontsource/jetbrains-mono/500.css',
        './src/styles/tokens.css',
        './src/styles/starlight-overrides.css',
      ],
      head: [
        {
          tag: 'meta',
          attrs: { property: 'og:image', content: ogUrl },
        },
        {
          tag: 'meta',
          attrs: { name: 'twitter:card', content: 'summary_large_image' },
        },
        {
          tag: 'meta',
          attrs: { name: 'twitter:image', content: ogUrl },
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/Esubaalew/appctl/edit/main/docs-next/',
      },
      lastUpdated: true,
      pagination: true,
      sidebar: [
        { label: 'Introduction', slug: 'docs/introduction' },
        { label: 'Installation', slug: 'docs/installation' },
        { label: 'First 10 minutes', slug: 'docs/first-10-minutes' },
        { label: 'Init', slug: 'docs/init' },
        { label: 'Provider matrix', slug: 'docs/provider-matrix' },
        { label: 'Quickstart', slug: 'docs/quickstart' },
        {
          label: 'Concepts',
          items: [
            { label: 'Mental model', slug: 'docs/concepts/mental-model' },
            { label: 'Sync and schema', slug: 'docs/concepts/sync-and-schema' },
            { label: 'Tools and actions', slug: 'docs/concepts/tools-and-actions' },
            { label: 'Agent loop', slug: 'docs/concepts/agent-loop' },
            { label: 'Provenance and safety', slug: 'docs/concepts/provenance-and-safety' },
          ],
        },
        {
          label: 'Sources',
          items: [
            { label: 'Choosing a sync source', slug: 'docs/sources/choosing-a-sync-source' },
            { label: 'OpenAPI / Swagger', slug: 'docs/sources/openapi' },
            { label: 'Django (DRF)', slug: 'docs/sources/django' },
            { label: 'Rails API', slug: 'docs/sources/rails' },
            { label: 'Laravel API', slug: 'docs/sources/laravel' },
            { label: 'ASP.NET Core', slug: 'docs/sources/aspnet' },
            { label: 'Strapi', slug: 'docs/sources/strapi' },
            { label: 'Supabase / PostgREST', slug: 'docs/sources/supabase' },
            { label: 'SQL databases', slug: 'docs/sources/db' },
            { label: 'URL login (HTML)', slug: 'docs/sources/url' },
            { label: 'MCP servers', slug: 'docs/sources/mcp' },
            { label: 'Plugins', slug: 'docs/sources/plugins' },
          ],
        },
        {
          label: 'CLI reference',
          items: [
            { label: 'setup', slug: 'docs/cli/setup' },
            { label: 'sync', slug: 'docs/cli/sync' },
            { label: 'chat', slug: 'docs/cli/chat' },
            { label: 'run', slug: 'docs/cli/run' },
            { label: 'doctor', slug: 'docs/cli/doctor' },
            { label: 'serve', slug: 'docs/cli/serve' },
            { label: 'mcp', slug: 'docs/cli/mcp' },
            { label: 'history', slug: 'docs/cli/history' },
            { label: 'config', slug: 'docs/cli/config' },
            { label: 'app', slug: 'docs/cli/app' },
            { label: 'plugin', slug: 'docs/cli/plugin' },
            { label: 'auth', slug: 'docs/cli/auth' },
          ],
        },
        {
          label: 'API',
          items: [
            { label: 'HTTP endpoints', slug: 'docs/api/http' },
            { label: 'WebSocket', slug: 'docs/api/websocket' },
            { label: 'AgentEvent stream', slug: 'docs/api/agent-events' },
          ],
        },
        {
          label: 'Deploy',
          items: [
            { label: 'Local use', slug: 'docs/deploy/local' },
            { label: 'Server', slug: 'docs/deploy/server' },
            { label: 'Docker', slug: 'docs/deploy/docker' },
            { label: 'Secrets and keys', slug: 'docs/deploy/secrets-and-keys' },
          ],
        },
        { label: 'Security', slug: 'docs/security' },
        { label: 'Troubleshooting', slug: 'docs/troubleshooting' },
        { label: 'Changelog', slug: 'docs/changelog' },
      ],
    }),
  ],
});
