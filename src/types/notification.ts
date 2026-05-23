export interface UpdateNotificationData {
  version?: string | null;
  current_version?: string | null;
  notes?: string | null;
  pub_date?: string | null;
  store_url?: string | null;
}

export interface AppNotification {
  id: number;
  kind: string;
  title: string;
  message: string;
  data?: UpdateNotificationData | null;
  is_read: boolean;
  created_at: string;
  updated_at: string;
}
