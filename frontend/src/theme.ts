import { useEffect, useState } from 'react';

export type ThemeId = 'magenta' | 'cyan' | 'synthwave' | 'emerald' | 'amber' | 'purple' | 'obsidian';

export interface ThemeConfig {
  id: ThemeId;
  name: string;
  shortName: string;
  primary: string;
  previewColor: string;
  glow: string;
  secondary: string;
  secondaryPreview: string;
  secondaryGlow: string;
  description: string;
}

export const THEMES: ThemeConfig[] = [
  {
    id: 'magenta',
    name: 'Neon Magenta & Cyber Cyan',
    shortName: 'Magenta + Cyan',
    primary: '#ff1493',
    previewColor: '#ff1493',
    glow: 'rgba(255, 20, 147, 0.5)',
    secondary: '#00f0ff',
    secondaryPreview: '#00f0ff',
    secondaryGlow: 'rgba(0, 240, 255, 0.5)',
    description: 'Hot Neon Magenta paired with Cyber Tron Cyan (Default)',
  },
  {
    id: 'cyan',
    name: 'Cyber Cyan & Neon Violet',
    shortName: 'Cyan + Violet',
    primary: '#00f0ff',
    previewColor: '#00f0ff',
    glow: 'rgba(0, 240, 255, 0.45)',
    secondary: '#c084fc',
    secondaryPreview: '#c084fc',
    secondaryGlow: 'rgba(192, 132, 252, 0.45)',
    description: 'Tron Neon Cyan paired with Electric Violet',
  },
  {
    id: 'synthwave',
    name: 'Synthwave Sunset & Amber',
    shortName: 'Sunset + Amber',
    primary: '#f43f5e',
    previewColor: '#f43f5e',
    glow: 'rgba(244, 63, 94, 0.45)',
    secondary: '#fbbf24',
    secondaryPreview: '#fbbf24',
    secondaryGlow: 'rgba(251, 191, 36, 0.45)',
    description: 'Hot Rose sunset paired with Solar Amber Gold',
  },
  {
    id: 'purple',
    name: 'Acid Violet & Matrix Green',
    shortName: 'Violet + Green',
    primary: '#c084fc',
    previewColor: '#c084fc',
    glow: 'rgba(192, 132, 252, 0.45)',
    secondary: '#00ff88',
    secondaryPreview: '#00ff88',
    secondaryGlow: 'rgba(0, 255, 136, 0.45)',
    description: 'Cyberpunk Electric Violet paired with Neon Emerald',
  },
  {
    id: 'emerald',
    name: 'Matrix Emerald & Cyan Glow',
    shortName: 'Emerald + Cyan',
    primary: '#00ff88',
    previewColor: '#00ff88',
    glow: 'rgba(0, 255, 136, 0.45)',
    secondary: '#00e5ff',
    secondaryPreview: '#00e5ff',
    secondaryGlow: 'rgba(0, 229, 255, 0.45)',
    description: 'Phosphor terminal green paired with Electric Cyan',
  },
  {
    id: 'amber',
    name: 'Solar Amber & Glacier Cyan',
    shortName: 'Amber + Cyan',
    primary: '#f59e0b',
    previewColor: '#fbbf24',
    glow: 'rgba(245, 158, 11, 0.45)',
    secondary: '#38bdf8',
    secondaryPreview: '#38bdf8',
    secondaryGlow: 'rgba(56, 189, 248, 0.45)',
    description: 'Retro CRT Amber paired with Glacier Sky Blue',
  },
  {
    id: 'obsidian',
    name: 'Obsidian Stealth & Ice Cyan',
    shortName: 'Stealth + Ice',
    primary: '#ffffff',
    previewColor: '#ffffff',
    glow: 'rgba(255, 255, 255, 0.35)',
    secondary: '#38bdf8',
    secondaryPreview: '#38bdf8',
    secondaryGlow: 'rgba(56, 189, 248, 0.35)',
    description: 'High-contrast monochrome White paired with Ice Cyan',
  },
];

const THEME_STORAGE_KEY = 'wirellm_theme';

export function getStoredTheme(): ThemeId {
  try {
    const saved = localStorage.getItem(THEME_STORAGE_KEY);
    if (saved && THEMES.some((t) => t.id === saved)) {
      return saved as ThemeId;
    }
  } catch (_) {}
  return 'magenta'; // Default to Neon Magenta!
}

export function applyTheme(theme: ThemeId): void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
    document.documentElement.setAttribute('data-theme', theme);
  } catch (_) {}
}

export function useTheme() {
  const [theme, setThemeState] = useState<ThemeId>(getStoredTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const setTheme = (newTheme: ThemeId) => {
    setThemeState(newTheme);
    applyTheme(newTheme);
  };

  const currentConfig = THEMES.find((t) => t.id === theme) || THEMES[0];

  return {
    theme,
    setTheme,
    themes: THEMES,
    currentConfig,
  };
}
