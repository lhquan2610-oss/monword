import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 0,
    rollupOptions: {
      output: {
        manualChunks(id) {
          const norm = id.replace(/\\/g, '/');
          if (norm.includes('/node_modules/')) {
            return 'vendor';
          }
          if (norm.includes('/src/data/')) {
            return 'rules-templates-data';
          }
          if (norm.includes('/src/utils/web')) {
            return 'web-engine';
          }
          if (norm.includes('/src/components/LivePreviewWorkspace') || norm.includes('/src/components/A4LivePreview')) {
            return 'preview-ui';
          }
          if (
            norm.includes('/src/components/HeaderBuilderWorkspace') ||
            norm.includes('/src/components/TemplateWorkspace') ||
            norm.includes('/src/components/StandardizationWorkspace') ||
            norm.includes('/src/components/LayoutIntelligenceWorkspace') ||
            norm.includes('/src/components/ComplianceAuditWorkspace')
          ) {
            return 'workspaces-ui';
          }
          return undefined;
        },
      },
    },
  },
});
