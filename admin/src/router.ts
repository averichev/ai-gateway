import { createRouter, createWebHistory } from 'vue-router'

import ModelRoutesView from './views/ModelRoutesView.vue'
import ProvidersView from './views/ProvidersView.vue'
import RequestDetailsView from './views/RequestDetailsView.vue'
import RequestsListView from './views/RequestsListView.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/requests',
    },
    {
      path: '/requests',
      name: 'requests',
      component: RequestsListView,
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
      path: '/model-routes',
      name: 'model-routes',
      component: ModelRoutesView,
    },
  ],
})

export default router
