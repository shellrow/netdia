import { createRouter, createWebHashHistory } from 'vue-router';

import Dashboard from '@/pages/Dashboard.vue';
import Interfaces from '@/pages/Interfaces.vue';
import Routes from '@/pages/Routes.vue';
import Internet from '@/pages/Internet.vue';
import Socket from '@/pages/Socket.vue';
import OsInfo from '@/pages/OS.vue';
import Settings from '@/pages/Settings.vue';
import DNS from '@/pages/DNS.vue';
import Ping from '@/pages/Ping.vue';
import Traceroute from '@/pages/Traceroute.vue';
import PortScan from '@/pages/PortScan.vue';
import HostScan from '@/pages/HostScan.vue';
import Neighbor from '@/pages/Neighbor.vue';
import TrafficMonitor from '@/pages/TrafficMonitor.vue';

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
