import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:8088',
      '/status': 'http://localhost:8088',
      '/logs': 'http://localhost:8088',
      '/v1': 'http://localhost:8088',
      '/mcp': 'http://localhost:8088',
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
