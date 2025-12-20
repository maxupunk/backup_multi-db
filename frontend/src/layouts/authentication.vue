<template>
  <v-main class="bg-background">
    <router-view />
  </v-main>
</template>

<script lang="ts" setup>
//
</script>

<style scoped>
.bg-background {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: rgb(var(--v-theme-surface-variant));
}
</style>
