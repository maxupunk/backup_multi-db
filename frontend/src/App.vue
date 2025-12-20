<template>
  <v-app>
    <router-view />
    <ReloadPrompt />
  </v-app>
</template>

<script lang="ts" setup>
  import ReloadPrompt from '@/components/common/ReloadPrompt.vue'
</script>
