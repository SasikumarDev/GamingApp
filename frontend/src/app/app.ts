import { Component, signal, OnDestroy } from '@angular/core';
import { UpperCasePipe } from '@angular/common';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { FormsModule } from '@angular/forms';
import type {
  SessionInfo,
  AuthResult,
  SessionStatus,
  SessionExpiredEvent,
  SessionStoppedEvent,
  AppState,
} from './types';

@Component({
  selector: 'app-root',
  imports: [FormsModule, UpperCasePipe],
  template: `
    @if (state() === 'expired') {
      <div class="expired-overlay">
        <div class="expired-box">
          <h1>Session Expired</h1>
          <p>{{ expiredReason() }}</p>
          <button (click)="reset()">Return to Login</button>
        </div>
      </div>
    } @else if (state() === 'connected') {
      <div class="session-bar">
        <span class="badge running">● {{ role() | uppercase }} Session</span>
        <span class="uptime">{{ uptime() }}</span>
        <button (click)="stop()" class="btn-stop">Stop</button>
      </div>
    } @else {
      <div class="login-container">
        <div class="card">
          <h2>Game Stream</h2>
          <div class="tabs">
            <button [class.active]="!isHost()" (click)="setMode(false)">Join</button>
            <button [class.active]="isHost()" (click)="setMode(true)">Host</button>
          </div>

          @if (isHost()) {
            @if (!sessionCreated()) {
              <button (click)="createSession()" [disabled]="loading()" class="btn-primary">
                {{ loading() ? 'Creating...' : 'Create Session' }}
              </button>
            } @else {
              <div class="password-display">
                <p class="label">Share this password with the client:</p>
                <div class="password-row">
                  <code>{{ sessionPassword() }}</code>
                  <button (click)="copyPassword()" class="btn-copy" title="Copy password">
                    {{ copied() ? 'Copied!' : 'Copy' }}
                  </button>
                </div>
              </div>
              <button (click)="startHost()" [disabled]="loading()" class="btn-primary">
                {{ loading() ? 'Starting...' : 'Start Streaming' }}
              </button>
            }
          } @else {
            <div class="form-group">
              <label>Host IP Address</label>
              <input [(ngModel)]="hostIp" placeholder="192.168.1.100" />
            </div>
            <div class="form-group">
              <label>Password</label>
              <input [(ngModel)]="password" type="password" placeholder="********" />
            </div>
            @if (authError()) {
              <p class="error-msg">{{ authError() }}</p>
            }
            <button (click)="connect()" [disabled]="loading() || !hostIp() || !password()" class="btn-primary">
              {{ loading() ? 'Connecting...' : 'Connect' }}
            </button>
          }
        </div>
      </div>
    }
  `,
  styles: [
    `
    .login-container {
      display: flex; align-items: center; justify-content: center;
      height: 100vh; background: #0f0f0f;
    }
    .card {
      background: #1a1a1a; border: 1px solid #333; border-radius: 12px;
      padding: 2rem; width: 360px; text-align: center;
    }
    .card h2 { margin: 0 0 1.5rem; color: #fff; font-weight: 600; }
    .tabs { display: flex; gap: 0; margin-bottom: 1.5rem; }
    .tabs button {
      flex: 1; padding: 0.5rem; border: 1px solid #333; background: #111;
      color: #888; cursor: pointer; font-size: 0.9rem;
    }
    .tabs button:first-child { border-radius: 6px 0 0 6px; }
    .tabs button:last-child { border-radius: 0 6px 6px 0; }
    .tabs button.active { background: #2563eb; color: #fff; border-color: #2563eb; }
    .password-display { margin-bottom: 1.5rem; }
    .password-display .label { font-size: 0.85rem; color: #888; margin-bottom: 0.5rem; }
    .password-row { display: flex; gap: 0.5rem; align-items: center; }
    .password-row code {
      flex: 1; font-size: 1.5rem; letter-spacing: 0.25em;
      background: #111; padding: 0.75rem; border-radius: 8px; color: #4ade80;
      overflow: hidden; text-overflow: ellipsis;
    }
    .btn-copy {
      padding: 0.5rem 1rem; border: 1px solid #555; border-radius: 6px;
      background: #222; color: #ccc; cursor: pointer; font-size: 0.85rem;
      white-space: nowrap; flex-shrink: 0;
    }
    .btn-copy:hover { background: #333; color: #fff; }
    .form-group { margin-bottom: 1rem; text-align: left; }
    .form-group label { display: block; font-size: 0.85rem; color: #888; margin-bottom: 0.3rem; }
    .form-group input {
      width: 100%; padding: 0.6rem; border: 1px solid #333; border-radius: 6px;
      background: #111; color: #e0e0e0; font-size: 0.95rem; box-sizing: border-box;
    }
    .form-group input:focus { outline: none; border-color: #2563eb; }
    .btn-primary {
      width: 100%; padding: 0.7rem; border: none; border-radius: 6px;
      background: #2563eb; color: #fff; font-size: 1rem; cursor: pointer; margin-top: 0.5rem;
    }
    .btn-primary:disabled { opacity: 0.4; cursor: not-allowed; }
    .error-msg { color: #ef4444; font-size: 0.85rem; margin: 0.5rem 0; }
    .expired-overlay {
      position: fixed; inset: 0; background: rgba(0,0,0,0.9);
      display: flex; align-items: center; justify-content: center; z-index: 9999;
    }
    .expired-box {
      background: #1a1a1a; border: 1px solid #ef4444; border-radius: 12px;
      padding: 2rem; text-align: center; max-width: 400px;
    }
    .expired-box h1 { color: #ef4444; margin: 0 0 1rem; }
    .expired-box p { color: #aaa; margin-bottom: 1.5rem; }
    .expired-box button {
      padding: 0.6rem 1.5rem; border: 1px solid #333; border-radius: 6px;
      background: #111; color: #e0e0e0; cursor: pointer;
    }
    .session-bar {
      display: flex; align-items: center; gap: 1rem;
      padding: 0.75rem 1rem; background: #1a1a1a; border-bottom: 1px solid #333;
    }
    .badge { font-size: 0.85rem; padding: 0.25rem 0.75rem; border-radius: 999px; }
    .badge.running { background: #14532d; color: #4ade80; }
    .uptime { color: #888; font-size: 0.85rem; margin-left: auto; }
    .btn-stop {
      padding: 0.35rem 1rem; border: 1px solid #ef4444; border-radius: 6px;
      background: transparent; color: #ef4444; cursor: pointer; font-size: 0.85rem;
    }
    .btn-stop:hover { background: #ef4444; color: #fff; }
  `],
})
export class App implements OnDestroy {
  protected state = signal<AppState>('disconnected');
  protected role = signal<'host' | 'client'>('client');
  protected sessionPassword = signal('');
  protected sessionCreated = signal(false);
  protected copied = signal(false);
  protected uptime = signal('');
  protected expiredReason = signal('');
  protected loading = signal(false);
  protected authError = signal('');
  protected isHost = signal(false);
  protected hostIp = signal('');
  protected password = signal('');

  private unlisteners: UnlistenFn[] = [];
  private uptimeInterval: ReturnType<typeof setInterval> | null = null;
  private startedAt = 0;

  constructor() {
    this.setupListeners();
    this.checkStatus();
  }

  async ngOnDestroy() {
    for (const un of this.unlisteners) un();
    if (this.uptimeInterval) clearInterval(this.uptimeInterval);
  }

  private async setupListeners() {
    this.unlisteners.push(
      await listen<SessionExpiredEvent>('session-expired', (e) => {
        this.expiredReason.set(e.payload.reason);
        this.state.set('expired');
        this.stopUptime();
      })
    );
    this.unlisteners.push(
      await listen<SessionStoppedEvent>('session-stopped', (_e) => {
        this.reset();
      })
    );
    this.unlisteners.push(
      await listen<{ success: boolean; error?: string }>('auth-result', (e) => {
        this.loading.set(false);
        if (e.payload.success) {
          this.state.set('connected');
          this.startUptime();
        } else {
          this.authError.set(e.payload.error ?? 'Authentication failed');
        }
      })
    );
    this.unlisteners.push(
      await listen<string>('connection-status', (e) => {
        console.log('Connection status:', e.payload);
      })
    );
  }

  setMode(host: boolean) {
    this.isHost.set(host);
    this.authError.set('');
    this.hostIp.set('');
    this.password.set('');
  }

  async createSession() {
    this.loading.set(true);
    try {
      const info = await invoke<SessionInfo>('host_create_session');
      this.sessionPassword.set(info.password);
      this.role.set('host');
      this.sessionCreated.set(true);
    } catch (e) {
      this.authError.set(`Failed: ${e}`);
    }
    this.loading.set(false);
  }

  async startHost() {
    this.loading.set(true);
    try {
      await invoke('start_host_session');
      this.state.set('connected');
      this.startedAt = Date.now();
      this.startUptime();
    } catch (e) {
      this.authError.set(`Failed: ${e}`);
    }
    this.loading.set(false);
  }

  async copyPassword() {
    try {
      await navigator.clipboard.writeText(this.sessionPassword());
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch (_e) { /* fallback: ignore */ }
  }

  async connect() {
    this.loading.set(true);
    this.authError.set('');
    try {
      const result = await invoke<AuthResult>('client_auth', {
        hostIp: this.hostIp(),
        password: this.password(),
      });
      if (result.success) {
        this.role.set('client');
        await invoke('start_client_session');
        this.state.set('connected');
        this.startedAt = Date.now();
        this.startUptime();
      } else {
        this.authError.set(result.error ?? 'Authentication failed');
      }
    } catch (e) {
      this.authError.set(`Connection failed: ${e}`);
    }
    this.loading.set(false);
  }

  async stop() {
    try {
      await invoke('stop_session');
    } catch (_e) { /* ignore */ }
    this.reset();
  }

  reset() {
    this.state.set('disconnected');
    this.loading.set(false);
    this.authError.set('');
    this.sessionPassword.set('');
    this.sessionCreated.set(false);
    this.copied.set(false);
    this.expiredReason.set('');
    this.isHost.set(false);
    this.stopUptime();
  }

  private async checkStatus() {
    try {
      const status = await invoke<SessionStatus>('session_status');
      if (status.active && status.role) {
        this.role.set(status.role);
        this.state.set('connected');
      }
    } catch (_e) { /* not in a session */ }
  }

  private startUptime() {
    this.startedAt = Date.now();
    this.updateUptime();
    this.uptimeInterval = setInterval(() => this.updateUptime(), 1000);
  }

  private stopUptime() {
    if (this.uptimeInterval) {
      clearInterval(this.uptimeInterval);
      this.uptimeInterval = null;
    }
  }

  private updateUptime() {
    const elapsed = Math.floor((Date.now() - this.startedAt) / 1000);
    const m = Math.floor(elapsed / 60);
    const s = elapsed % 60;
    this.uptime.set(`${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`);
  }
}
