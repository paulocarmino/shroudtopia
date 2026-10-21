# Shroudtopia — kit V2, com arte ilustrada

**Este kit substitui o anterior.** A paisagem, o santuário e o medalhão são arquivos gerados de verdade. A marca também foi corrigida: símbolo e nome centralizados entre si, com mais espaço.

1. Extraia os arquivos desta pasta para a raiz do app/mod.
2. Substitua a marca anterior pelos arquivos `shroudtopia-logo.svg` / `.png` deste pacote.
3. Passe ao Claude o pedido abaixo, junto do projeto.

> Leia SHROUDTOPIA_ART_DIRECTION.md deste pacote V2 e integre as três artes fornecidas ao app existente. O guia anterior foi substituído porque o resultado ficou restrito à troca de cores. Use o cenário visível na área inferior livre, o santuário na sidebar e o medalhão no cabeçalho do mundo. Use a assinatura corrigida, com alinhamento central e espaço entre símbolo e nome. Preserve telas, navegação e regras de funcionamento. Adapte os recursos ao empacotador real e verifique o resultado no app.

## Conteúdo

- `shroudtopia-mist-valley.png`: cenário opaco, 1536 × 1024 px.
- `shroudtopia-flame-shrine.png`: santuário recortado, 1199 × 1312 px, com transparência.
- `shroudtopia-world-medallion.png`: emblema recortado, 1254 × 1254 px, com transparência.
- `shroudtopia-logo.svg` / `.png`: assinatura V2, com intervalo e alinhamento corrigidos.
- `shroudtopia-mark.svg` / `.png`: símbolo sozinho, mantido para usos compactos.
- `shroudtopia-theme.css`: paleta e estados de referência.
- `shroudtopia-art.css`: referência de aplicação das artes e da marca.
- `SHROUDTOPIA_ART_DIRECTION.md`: guia completo para implementar.
- `PROMPTS_GERACAO.md`: prompts exatos das três ilustrações e informações de geração.

Os arquivos CSS são referências, não uma alteração já aplicada ao código. O projeto precisa resolver os caminhos dos recursos. Não foi necessário nem criado um redesenho das telas.
