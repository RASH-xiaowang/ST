import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'node:path';

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: ['browser'],
    alias: {
      '@wechat': path.resolve(__dirname, 'src/lib/wechat'),
      '$lib': path.resolve(__dirname, 'src/lib'),
      'src': path.resolve(__dirname, 'src'),
    },
  },
  test: {
    include: ['src/**/*.test.ts', 'src/**/*.spec.ts'],
    environment: 'jsdom',
  },
});
