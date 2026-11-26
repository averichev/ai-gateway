import { createRouter, createWebHistory } from 'vue-router'

import { useAuth } from './auth'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/requests',
    },
    {
      path: '/login',
      name: 'login',
      component: () => import('./views/LoginView.vue'),
      meta: { public: true },
    },
    {
      path: '/requests',
      name: 'requests',
      component: () => import('./views/RequestsListView.vue'),
    },
    {
      path: '/generate-test',
      name: 'generate-test',
      component: () => import('./views/GenerateTestView.vue'),
    },
    {
      path: '/requests/:id',
      name: 'request-details',
      component: () => import('./views/RequestDetailsView.vue'),
      props: true,
    },
    {
      path: '/providers',
      name: 'providers',
      component: () => import('./views/ProvidersView.vue'),
    },
    {
      path: '/gateway-clients',
      name: 'gateway-clients',
      component: () => import('./views/GatewayClientsView.vue'),
    },
    {
      path: '/model-routes',
      name: 'model-routes',
      component: () => import('./views/ModelRoutesView.vue'),
    },
    {
      path: '/tenants',
      name: 'tenants',
      component: () => import('./views/TenantsView.vue'),
    },
    {
      path: '/users',
      name: 'users',
      component: () => import('./views/UsersView.vue'),
    },
    {
      path: '/updates',
      component: () => import('./views/UpdatesView.vue'),
      meta: { ownerOnly: true },
    },
  ],
})

router.beforeEach(async (to) => {
  const auth = useAuth()
  await auth.initAuth()

  if (to.meta.public) {
    return auth.isAuthenticated.value ? { path: '/requests' } : true
  }

  if (!auth.isAuthenticated.value) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }

  if (to.meta.ownerOnly && auth.user.value?.global_role !== 'owner') {
    return { path: '/requests' }
  }

  return true
})

export default router
