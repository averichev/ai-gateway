export interface MenuSection {
  label: string
  icon?: string
  to?: string
  url?: string
  target?: string
  disabled?: boolean
  class?: string
  path?: string
  visible?: boolean
  items?: MenuSection[]
}
