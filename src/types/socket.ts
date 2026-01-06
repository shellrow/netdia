
export type TcpState =
  | "Closed"
  | "Listen"
  | "SynSent"
  | "SynRecv"
  | "Established"
  | "FinWait1"
  | "FinWait2"
  | "CloseWait"
  | "Closing"
  | "LastAck"
  | "TimeWait"
  | "Unknown";

export type ProcessEntry = {
  pid: number;
  name: string;
};

export type TcpSocketInfo = {
  local_addr: string;
  local_port: number;
  remote_addr: string;
  remote_port: number;
  state: TcpState;
};

export type UdpSocketInfo = {
  local_addr: string;
  local_port: number;
};

export type ProtocolSocketInfo =
  | { Tcp: TcpSocketInfo }
  | { Udp: UdpSocketInfo };

export type SocketInfo = {
  protocol_socket_info: ProtocolSocketInfo;
  processes: ProcessEntry[];
  inode: number;
  uid: number;
};
