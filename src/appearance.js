export const defaultAppearanceConfig = {
  opacity: 92,
  borderRadius: 12,
  themeMode: 'system',
  accentColor: '#7dd3fc',
  fontSize: 'medium',
  backgroundBlur: true,
  animations: true,
  windowShadow: false
}

const themeModes = new Set(['system', 'light', 'dark'])
const fontSizes = new Set(['small', 'medium', 'large'])
const fontScaleBySize = {
  small: 0.92,
  medium: 1,
  large: 1.1
}

export function cloneAppearanceConfig(config) {
  return { ...normalizeAppearanceConfig(config) }
}

export function normalizeAppearanceConfig(config = {}) {
  const next = { ...defaultAppearanceConfig, ...config }

  return {
    opacity: clampNumber(next.opacity, 40, 100, defaultAppearanceConfig.opacity),
    borderRadius: clampNumber(
      next.borderRadius,
      0,
      32,
      defaultAppearanceConfig.borderRadius
    ),
    themeMode: themeModes.has(next.themeMode) ? next.themeMode : defaultAppearanceConfig.themeMode,
    accentColor: isHexColor(next.accentColor) ? next.accentColor : defaultAppearanceConfig.accentColor,
    fontSize: fontSizes.has(next.fontSize) ? next.fontSize : defaultAppearanceConfig.fontSize,
    backgroundBlur: Boolean(next.backgroundBlur),
    animations: Boolean(next.animations),
    windowShadow: Boolean(next.windowShadow)
  }
}

export function configsEqual(left, right) {
  const a = normalizeAppearanceConfig(left)
  const b = normalizeAppearanceConfig(right)

  return Object.keys(defaultAppearanceConfig).every((key) => a[key] === b[key])
}

export function applyAppearanceConfig(config) {
  const next = normalizeAppearanceConfig(config)
  const root = document.documentElement
  const resolvedTheme = resolveThemeMode(next.themeMode)

  root.dataset.themeMode = next.themeMode
  root.dataset.resolvedTheme = resolvedTheme
  root.style.setProperty('--panel-opacity', (next.opacity / 100).toFixed(2))
  root.style.setProperty('--panel-radius', `${next.borderRadius}px`)
  root.style.setProperty('--accent', next.accentColor)
  root.style.setProperty('--font-scale', fontScaleBySize[next.fontSize].toString())
  root.style.setProperty('--panel-blur', next.backgroundBlur ? 'blur(18px)' : 'none')
  root.style.setProperty(
    '--panel-shadow',
    next.windowShadow ? '0 16px 40px rgb(0 0 0 / 0.34)' : 'none'
  )
  root.style.setProperty('--motion-duration', next.animations ? '160ms' : '0ms')
}

function resolveThemeMode(themeMode) {
  if (themeMode === 'light' || themeMode === 'dark') {
    return themeMode
  }

  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

function clampNumber(value, min, max, fallback) {
  const number = Number(value)
  if (!Number.isFinite(number)) {
    return fallback
  }

  return Math.min(Math.max(Math.round(number), min), max)
}

function isHexColor(value) {
  return typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value)
}
