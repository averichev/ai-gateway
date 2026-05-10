<script setup lang="ts">
const items = [
  { label: 'Запросы', icon: 'pi pi-list', route: '/requests' },
  { label: 'Проверка', icon: 'pi pi-send', route: '/generate-test' },
  { label: 'Providers', icon: 'pi pi-server', route: '/providers' },
  { label: 'Model Routes', icon: 'pi pi-sitemap', route: '/model-routes' },
]
</script>

<template>
  <div class="layout-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-title">AI Gateway</div>
        <div class="brand-subtitle">MVP Admin</div>
      </div>

      <nav class="nav-links">
        <RouterLink
          v-for="item in items"
          :key="item.route"
          :to="item.route"
          class="nav-link"
          active-class="nav-link-active"
        >
          <i :class="item.icon" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </nav>
    </aside>

    <main class="page-content">
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.layout-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  background: #f5f7fb;
  color: #172033;
}

.sidebar {
  padding: 1.5rem 1rem;
  background: #111827;
  color: #f8fafc;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.brand {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.brand-title {
  font-size: 1.25rem;
  font-weight: 700;
}

.brand-subtitle {
  color: #94a3b8;
  font-size: 0.95rem;
}

.nav-links {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.nav-link {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  border-radius: 0.75rem;
  padding: 0.8rem 1rem;
  color: #cbd5e1;
  text-decoration: none;
}

.nav-link-active,
.nav-link:hover {
  background: rgba(255, 255, 255, 0.08);
  color: #ffffff;
}

.page-content {
  min-width: 0;
  padding: 2rem;
}

:global(body) {
  margin: 0;
  font-family: Inter, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

@media (max-width: 900px) {
  .layout-shell {
    grid-template-columns: 1fr;
  }

  .sidebar {
    gap: 1rem;
  }

  .nav-links {
    flex-direction: row;
    flex-wrap: wrap;
  }

  .page-content {
    padding: 1rem;
  }
}
</style>
