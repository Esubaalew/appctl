/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#000000",
        surface: "#0a0a0a",
        panel: "#121212",
        "panel-2": "#171717",
        elev: "#1e1e1e",
        border: "#262626",
        "border-strong": "#3f3f46",
        fg: "#ffffff",
        "fg-dim": "#a1a1aa",
        muted: "#71717a",
        "muted-2": "#52525b",
        accent: "#ffffff",
        "accent-2": "#a1a1aa",
        success: "#34d399",
        warn: "#fbbf24",
        danger: "#f87171",
      },
      fontFamily: {
        sans: [
          "Inter",
          "ui-sans-serif",
          "system-ui",
          "-apple-system",
          "Segoe UI",
          "Roboto",
          "sans-serif",
        ],
        mono: [
          "JetBrains Mono",
          "ui-monospace",
          "SF Mono",
          "Menlo",
          "Monaco",
          "Consolas",
          "monospace",
        ],
      },
      boxShadow: {
        panel: "0 1px 2px rgba(0,0,0,0.5)",
        "panel-strong": "0 4px 12px rgba(0,0,0,0.8)",
        glow: "0 0 0 1px rgba(255,255,255,0.1)",
      },
      animation: {
        "pulse-dot": "pulseDot 1.4s ease-in-out infinite",
        shimmer: "shimmer 2s linear infinite",
      },
      keyframes: {
        pulseDot: {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.35" },
        },
        shimmer: {
          "0%": { backgroundPosition: "-200% 0" },
          "100%": { backgroundPosition: "200% 0" },
        },
      },
    },
  },
  plugins: [],
};
