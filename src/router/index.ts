import { createRouter, createWebHashHistory } from 'vue-router';

const Dashboard = () => import('@/pages/Dashboard.vue');
const Interfaces = () => import('@/pages/Interfaces.vue');
const Routes = () => import('@/pages/Routes.vue');
const Internet = () => import('@/pages/Internet.vue');
const Socket = () => import('@/pages/Socket.vue');
const OsInfo = () => import('@/pages/OS.vue');
const Settings = () => import('@/pages/Settings.vue');
const DNS = () => import('@/pages/DNS.vue');
const Ping = () => import('@/pages/Ping.vue');
const Traceroute = () => import('@/pages/Traceroute.vue');
const PortScan = () => import('@/pages/PortScan.vue');
const HostScan = () => import('@/pages/HostScan.vue');
const Neighbor = () => import('@/pages/Neighbor.vue');
const TrafficMonitor = () => import('@/pages/TrafficMonitor.vue');

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: { name: 'dashboard' } },
    { path: '/dashboard', name: 'dashboard', component: Dashboard },
    { path: '/interfaces', name: 'interfaces', component: Interfaces },
    { path: '/monitor', name: 'monitor', component: TrafficMonitor },
    { path: '/routes', name: 'routes', component: Routes },
    { path: '/neighbor', name: 'neighbor', component: Neighbor },
    { path: '/internet', name: 'internet', component: Internet },
    { path: '/socket', name: 'socket', component: Socket },
    { path: '/os', name: 'os', component: OsInfo },
    { path: '/dns', name: 'dns', component: DNS },
    { path: '/ping', name: 'ping', component: Ping },
    { path: '/traceroute', name: 'traceroute', component: Traceroute },
    { path: '/portscan', name: 'portscan', component: PortScan },
    { path: '/hostscan', name: 'hostscan', component: HostScan },
    { path: '/settings', name: 'settings', component: Settings },
    { path: '/:pathMatch(.*)*', redirect: { name: 'dashboard' } },
  ],
});

export default router;
