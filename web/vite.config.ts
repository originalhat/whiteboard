import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: { "/api": `http://127.0.0.1:${process.env.WB_PORT ?? 4747}` },
  },
  define: { "process.env.IS_PREACT": JSON.stringify("false") },
  test: { environment: "node" },
});
