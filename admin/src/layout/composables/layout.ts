import { computed, reactive } from 'vue'

const layoutConfig = reactive({
  darkTheme: false,
  menuMode: 'static',
})

const layoutState = reactive({
  staticMenuInactive: false,
  overlayMenuActive: false,
  mobileMenuActive: false,
  menuHoverActive: false,
  activePath: null as string | null,
})

export function useLayout() {
  function executeDarkModeToggle() {
    layoutConfig.darkTheme = !layoutConfig.darkTheme
    document.documentElement.classList.toggle('app-dark', layoutConfig.darkTheme)
  }

  function toggleDarkMode() {
    executeDarkModeToggle()
  }

  function isDesktop() {
    return window.innerWidth > 991
  }

  function toggleMenu() {
    if (isDesktop()) {
      if (layoutConfig.menuMode === 'static') {
        layoutState.staticMenuInactive = !layoutState.staticMenuInactive
      } else {
        layoutState.overlayMenuActive = !layoutState.overlayMenuActive
      }
    } else {
      layoutState.mobileMenuActive = !layoutState.mobileMenuActive
    }
  }

  function hideMobileMenu() {
    layoutState.mobileMenuActive = false
  }

  const isDarkTheme = computed(() => layoutConfig.darkTheme)
  const hasOpenOverlay = computed(() => layoutState.overlayMenuActive)

  return {
    layoutConfig,
    layoutState,
    isDarkTheme,
    toggleDarkMode,
    toggleMenu,
    hideMobileMenu,
    isDesktop,
    hasOpenOverlay,
  }
}
