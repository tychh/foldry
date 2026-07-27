import { createTheme, rem } from "@mantine/core";

export const foldryTheme = createTheme({
  primaryColor: "blue",
  primaryShade: { light: 7, dark: 5 },
  defaultRadius: "sm",
  fontFamily:
    "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif",
  fontFamilyMonospace:
    "IBM Plex Mono, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
  headings: {
    fontFamily:
      "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif",
    fontWeight: "650",
    sizes: {
      h1: { fontSize: rem(28), lineHeight: "1.2" },
      h2: { fontSize: rem(19), lineHeight: "1.3" },
      h3: { fontSize: rem(16), lineHeight: "1.35" },
    },
  },
  spacing: {
    xs: rem(8),
    sm: rem(12),
    md: rem(16),
    lg: rem(24),
    xl: rem(32),
  },
  radius: {
    xs: rem(4),
    sm: rem(6),
    md: rem(9),
    lg: rem(12),
    xl: rem(16),
  },
  shadows: {
    xs: "0 1px 2px rgb(16 24 40 / 0.04)",
    sm: "0 2px 8px rgb(16 24 40 / 0.06)",
    md: "0 8px 24px rgb(16 24 40 / 0.08)",
    lg: "0 16px 40px rgb(16 24 40 / 0.1)",
    xl: "0 24px 64px rgb(16 24 40 / 0.12)",
  },
});
