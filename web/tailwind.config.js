/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0b0d10",
        fg: "#e7ecf2",
        muted: "#8a94a4",
        accent: "#7cc4ff",
        accent2: "#a8ff9a",
        panel: "#141820",
        border: "#222833",
        code: "#0f1319",
      },
      fontFamily: {
        sans: [
          "ui-sans-serif",
          "system-ui",
          "-apple-system",
          "Segoe UI",
          "Roboto",
          "Inter",
          "sans-serif",
        ],
        mono: ["ui-monospace", "SF Mono", "Menlo", "Monaco", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
