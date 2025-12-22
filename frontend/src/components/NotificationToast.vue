<template>
    <div class="notification-container">
        <transition-group name="notification-list" tag="div">
            <v-alert v-for="notification in store.notifications" :key="notification.id" :type="notification.type"
                :title="notification.title" class="mb-2 notification-item" closable elevation="4" max-width="400"
                border="start" density="comfortable" variant="tonal" @click:close="store.remove(notification.id)">
                <template v-slot:text>
                    <div class="text-body-2">{{ notification.message }}</div>
                    <div class="text-caption text-medium-emphasis mt-1 d-flex justify-end">
                        {{ formatTime(notification.timestamp) }}
                    </div>
                </template>
            </v-alert>
        </transition-group>
    </div>
</template>

<script setup lang="ts">
import { useNotificationStore } from '@/stores/notification'

const store = useNotificationStore()

function formatTime(isoString: string) {
    try {
        return new Date(isoString).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
    } catch (e) {
        return isoString
    }
}
</script>

<style scoped>
.notification-container {
    position: fixed;
    top: 24px;
    right: 24px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    pointer-events: none;
    /* Permite clicar através do container vazio */
    width: 400px;
    max-width: calc(100vw - 48px);
}

.notification-item {
    pointer-events: auto;
    /* Reativa cliques nos alertas */
    margin-bottom: 8px;
    backdrop-filter: blur(5px);
    background-color: rgba(var(--v-theme-surface), 0.95);
}

/* Transições */
.notification-list-enter-active,
.notification-list-leave-active {
    transition: all 0.4s cubic-bezier(0.4, 0, 0.2, 1);
}

.notification-list-enter-from {
    opacity: 0;
    transform: translateX(50px) scale(0.9);
}

.notification-list-leave-to {
    opacity: 0;
    transform: translateX(50px);
}
</style>
