import { cpSync, rmSync } from "node:fs";

const from = "node_modules/@excalidraw/excalidraw/dist/prod/fonts";
rmSync("public/fonts", { recursive: true, force: true });
cpSync(from, "public/fonts", {
  recursive: true,
  filter: (path) => !path.includes("Xiaolai"),
});
