import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import dts from 'vite-plugin-dts';
import { resolve } from 'path';

export default defineConfig({
  plugins: [
    react(),
    dts({
      insertTypesEntry: true,
    }),
  ],
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.tsx'),
      name: 'TulipWidgets',
      formats: ['iife'],
      fileName: () => 'tulip-game-widgets.js',
    },
    rollupOptions: {
      // Bundle everything - no external dependencies
      // React will be bundled in
      output: {
        // Expose as window.TulipWidgets
        extend: true,
        exports: 'named',
      },
    },
    // Generate sourcemap for debugging
    sourcemap: true,
    // Minify for production
    minify: 'terser',
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
});
