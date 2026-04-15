import { invoke } from '@tauri-apps/api/core';

export interface InterfaceDetails {
  name: string;
  ip: string | null;
  subnet: string | null;
  gateway: string | null;
  dns: string[];
  dhcp_enabled: boolean;
  is_up: boolean;
}

export interface ApplyStaticArgs {
  interface: string;
  ip: string;
  subnet: string;
  gateway: string;
  dns_primary: string;
  dns_secondary?: string;
}

export const api = {
  listInterfaces: () => invoke<string[]>('cmd_list_interfaces'),
  getInterfaceDetails: (name: string) => invoke<InterfaceDetails>('cmd_get_interface_details', { name }),
  applyStatic: (args: ApplyStaticArgs) => invoke<void>('cmd_apply_static', { args }),
  applyDhcp: (iface: string) => invoke<void>('cmd_apply_dhcp', { interface: iface }),
};
