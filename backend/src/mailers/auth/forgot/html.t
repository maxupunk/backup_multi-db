<!doctype html>
<html lang="pt-BR">
  <body style="font-family: system-ui, sans-serif; line-height: 1.5; color: #1f2430;">
    <p>Olá, {{ name }}.</p>

    <p>Recebemos um pedido de redefinição de senha para esta conta.</p>

    <p>
      <a href="{{ resetUrl }}"
         style="display: inline-block; padding: 10px 18px; background: #1867c0; color: #fff; border-radius: 4px; text-decoration: none;">
        Escolher uma senha nova
      </a>
    </p>

    <p style="font-size: 13px; color: #5a6172;">
      Se o botão não funcionar, copie este endereço no navegador:<br />
      <span>{{ resetUrl }}</span>
    </p>

    <p style="font-size: 13px; color: #5a6172;">
      O link vale por 4 horas e só pode ser usado uma vez. Se não foi você quem
      pediu, ignore esta mensagem — sua senha atual continua valendo.
    </p>
  </body>
</html>
