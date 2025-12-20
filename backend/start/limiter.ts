/*
|--------------------------------------------------------------------------
| Rate Limiter Preload
|--------------------------------------------------------------------------
|
| This file is loaded before the application starts.
| It ensures the limiter service is initialized.
|
*/

// Just importing the limiter service ensures it's initialized
import '@adonisjs/limiter/services/main'
