import '@adonisjs/core/types/http'

type ParamValue = string | number | bigint | boolean

export type ScannedRoutes = {
  ALL: {
    'auth.check_status': { paramsTuple?: []; params?: {} }
    'auth.register': { paramsTuple?: []; params?: {} }
    'auth.login': { paramsTuple?: []; params?: {} }
    'auth.me': { paramsTuple?: []; params?: {} }
    'auth.logout': { paramsTuple?: []; params?: {} }
    'connections.discover_databases': { paramsTuple?: []; params?: {} }
    'connections.docker_hosts': { paramsTuple?: []; params?: {} }
    'connections.index': { paramsTuple?: []; params?: {} }
    'connections.store': { paramsTuple?: []; params?: {} }
    'connections.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.index': { paramsTuple?: []; params?: {} }
    'storage_destinations.store': { paramsTuple?: []; params?: {} }
    'storage_destinations.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.space_all': { paramsTuple?: []; params?: {} }
    'storage_destinations.space': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.index': { paramsTuple?: []; params?: {} }
    'storages.store': { paramsTuple?: []; params?: {} }
    'storages.copy_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.archive_job_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.download_archive': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.test': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.browse': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.start_copy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.start_archive': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.test': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.create_database': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.backup': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.index': { paramsTuple?: []; params?: {} }
    'backups.by_connection': { paramsTuple: [ParamValue]; params: {'connectionId': ParamValue} }
    'backups.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.download': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.restore': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.import': { paramsTuple?: []; params?: {} }
    'system.stats': { paramsTuple?: []; params?: {} }
    'system.status': { paramsTuple?: []; params?: {} }
    'system.container_resources': { paramsTuple?: []; params?: {} }
    'system.resources_history': { paramsTuple?: []; params?: {} }
    'audit_logs.index': { paramsTuple?: []; params?: {} }
    'audit_logs.stats': { paramsTuple?: []; params?: {} }
    'audit_logs.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'users.index': { paramsTuple?: []; params?: {} }
    'users.toggle_status': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_containers': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.container_logs': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.start_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.stop_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.restart_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_volumes': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_volume': { paramsTuple: [ParamValue]; params: {'name': ParamValue} }
    'docker_manager.remove_volume': { paramsTuple: [ParamValue]; params: {'name': ParamValue} }
    'docker_manager.list_networks': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_network': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.prune_images': { paramsTuple?: []; params?: {} }
    'docker_manager.list_images': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_image': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.remove_image': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'event_stream': { paramsTuple?: []; params?: {} }
    'subscribe': { paramsTuple?: []; params?: {} }
    'unsubscribe': { paramsTuple?: []; params?: {} }
  }
  GET: {
    'auth.check_status': { paramsTuple?: []; params?: {} }
    'auth.me': { paramsTuple?: []; params?: {} }
    'connections.docker_hosts': { paramsTuple?: []; params?: {} }
    'connections.index': { paramsTuple?: []; params?: {} }
    'connections.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.index': { paramsTuple?: []; params?: {} }
    'storage_destinations.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.space_all': { paramsTuple?: []; params?: {} }
    'storage_destinations.space': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.index': { paramsTuple?: []; params?: {} }
    'storages.copy_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.archive_job_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.download_archive': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.browse': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.index': { paramsTuple?: []; params?: {} }
    'backups.by_connection': { paramsTuple: [ParamValue]; params: {'connectionId': ParamValue} }
    'backups.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.download': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'system.stats': { paramsTuple?: []; params?: {} }
    'system.status': { paramsTuple?: []; params?: {} }
    'system.container_resources': { paramsTuple?: []; params?: {} }
    'system.resources_history': { paramsTuple?: []; params?: {} }
    'audit_logs.index': { paramsTuple?: []; params?: {} }
    'audit_logs.stats': { paramsTuple?: []; params?: {} }
    'audit_logs.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'users.index': { paramsTuple?: []; params?: {} }
    'docker_manager.list_containers': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.container_logs': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_volumes': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_volume': { paramsTuple: [ParamValue]; params: {'name': ParamValue} }
    'docker_manager.list_networks': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_network': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_images': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_image': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'event_stream': { paramsTuple?: []; params?: {} }
  }
  HEAD: {
    'auth.check_status': { paramsTuple?: []; params?: {} }
    'auth.me': { paramsTuple?: []; params?: {} }
    'connections.docker_hosts': { paramsTuple?: []; params?: {} }
    'connections.index': { paramsTuple?: []; params?: {} }
    'connections.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.index': { paramsTuple?: []; params?: {} }
    'storage_destinations.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.space_all': { paramsTuple?: []; params?: {} }
    'storage_destinations.space': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.index': { paramsTuple?: []; params?: {} }
    'storages.copy_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.archive_job_status': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.download_archive': { paramsTuple: [ParamValue]; params: {'jobId': ParamValue} }
    'storages.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.browse': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.index': { paramsTuple?: []; params?: {} }
    'backups.by_connection': { paramsTuple: [ParamValue]; params: {'connectionId': ParamValue} }
    'backups.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.download': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'system.stats': { paramsTuple?: []; params?: {} }
    'system.status': { paramsTuple?: []; params?: {} }
    'system.container_resources': { paramsTuple?: []; params?: {} }
    'system.resources_history': { paramsTuple?: []; params?: {} }
    'audit_logs.index': { paramsTuple?: []; params?: {} }
    'audit_logs.stats': { paramsTuple?: []; params?: {} }
    'audit_logs.show': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'users.index': { paramsTuple?: []; params?: {} }
    'docker_manager.list_containers': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.container_logs': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_volumes': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_volume': { paramsTuple: [ParamValue]; params: {'name': ParamValue} }
    'docker_manager.list_networks': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_network': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.list_images': { paramsTuple?: []; params?: {} }
    'docker_manager.inspect_image': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'event_stream': { paramsTuple?: []; params?: {} }
  }
  POST: {
    'auth.register': { paramsTuple?: []; params?: {} }
    'auth.login': { paramsTuple?: []; params?: {} }
    'auth.logout': { paramsTuple?: []; params?: {} }
    'connections.discover_databases': { paramsTuple?: []; params?: {} }
    'connections.store': { paramsTuple?: []; params?: {} }
    'storage_destinations.store': { paramsTuple?: []; params?: {} }
    'storages.store': { paramsTuple?: []; params?: {} }
    'storages.test': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.start_copy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.start_archive': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.test': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.create_database': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'connections.backup': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.restore': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.import': { paramsTuple?: []; params?: {} }
    'docker_manager.start_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.stop_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.restart_container': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.prune_images': { paramsTuple?: []; params?: {} }
    'subscribe': { paramsTuple?: []; params?: {} }
    'unsubscribe': { paramsTuple?: []; params?: {} }
  }
  PUT: {
    'connections.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
  }
  PATCH: {
    'connections.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.update': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'users.toggle_status': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
  }
  DELETE: {
    'connections.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storage_destinations.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'storages.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'backups.destroy': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
    'docker_manager.remove_volume': { paramsTuple: [ParamValue]; params: {'name': ParamValue} }
    'docker_manager.remove_image': { paramsTuple: [ParamValue]; params: {'id': ParamValue} }
  }
}
declare module '@adonisjs/core/types/http' {
  export interface RoutesList extends ScannedRoutes {}
}