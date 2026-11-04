import { defineConfig } from "vite";
import { readCurrentPreset } from "./tools/preset-source";
export default defineConfig({
  publicDir: false,
  server: {
    host: "127.0.0.1",
    port: 5178,
    strictPort: true,
    fs: { strict: true },
  },
  plugins: [
    {
      name: "panzer-explicit-native-preset",
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          if (req.url?.split("?")[0] !== "/native-preset.json") return next();
          if (req.method !== "GET" && req.method !== "HEAD") {
            res.statusCode = 405;
            res.end();
            return;
          }
          try {
            const preset = readCurrentPreset();
            res.setHeader("Content-Type", "application/json");
            res.setHeader("Cache-Control", "no-store");
            res.setHeader("X-Panzer-Preset-Source", preset.source);
            res.end(req.method === "HEAD" ? undefined : preset.text);
          } catch {
            res.statusCode = 503;
            res.end("Preset unavailable");
          }
        });
      },
      generateBundle() {
        const preset = readCurrentPreset();
        this.emitFile({
          type: "asset",
          fileName: "native-preset.json",
          source: preset.text,
        });
        this.info(`Packaged ${preset.source} preset snapshot`);
      },
    },
  ],
});
