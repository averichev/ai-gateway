import { createRouter, createWebHistory } from 'vue-router'

import { useAuth } from './auth'
import GatewayClientsView from './views/GatewayClientsView.vue'
import LoginView from './views/LoginView.vue'
import ModelRoutesView from './views/ModelRoutesView.vue'
import ProvidersView from './views/ProvidersView.vue'
import GenerateTestView from './views/GenerateTestView.vue'
import RequestDetailsView from './views/RequestDetailsView.vue'
import RequestsListView from './views/RequestsListView.vue'
import TenantsView from './views/TenantsView.vue'
import UsersView from './views/UsersView.vue'

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
      component: LoginView,
      meta: { public: true },
    },
    {
      path: '/requests',
      name: 'requests',
      component: RequestsListView,
    },
    {
      path: '/generate-test',
      name: 'generate-test',
      component: GenerateTestView,
    },
    {
      path: '/requests/:id',
      name: 'request-details',
      component: RequestDetailsView,
      props: true,
    },
    {
      path: '/providers',
      name: 'providers',
      component: ProvidersView,
    },
    {
      path: '/gateway-clients',
      name: 'gateway-clients',
      component: GatewayClientsView,
    },
    {
      path: '/model-routes',
      name: 'model-routes',
      component: ModelRoutesView,
    },
    {
      path: '/tenants',
      name: 'tenants',
      component: TenantsView,
    },
    {
      path: '/users',
      name: 'users',
      component: UsersView,
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

  return true
})

export default router
