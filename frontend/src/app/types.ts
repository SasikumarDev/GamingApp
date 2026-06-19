import type { invoke } from '@tauri-apps/api/core';

export interface SessionInfo {
  password: string;
  token: string;
  expires_at: number;
}

export interface AuthResult {
  success: boolean;
  error?: string;
}

export interface SessionStatus {
  active: boolean;
  role: 'host' | 'client' | null;
  connected_peers: number;
  uptime_seconds: number;
}

export interface SessionExpiredEvent {
  reason: string;
}

export interface SessionStartedEvent {
  role: 'host' | 'client';
}

export interface SessionStoppedEvent {
  reason: string;
}

export interface AuthResultEvent {
  success: boolean;
  error?: string;
}

export interface ConnectionStatusEvent {
  status: string;
}

export type AppState = 'disconnected' | 'connecting' | 'connected' | 'expired';
