import { createApp } from 'vue'
import { createI18n } from 'vue-i18n'
import { createRouter, createWebHashHistory } from 'vue-router'
import '../../src/style.css'
import Preview from './Preview.vue'
const router = createRouter({ history: createWebHashHistory(), routes: [{path: '/:pathMatch(.*)*', component: {template: '<div />'}}] })
createApp(Preview).use(router).use(createI18n({ legacy: false, locale: 'en', messages: {en: {common: {done: 'Done', copy: 'Copy'}, transactions: {title: 'Transactions', unconfirmed: 'Pending', minutesAgo: '{count}m ago', confirmations: '{count} confirmations'}}} })).mount('#app')
