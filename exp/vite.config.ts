import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // The live app's screenshots are read from ../target/screenshots.
  server: { port: 5178, fs: { allow: ['..'] } },
})
