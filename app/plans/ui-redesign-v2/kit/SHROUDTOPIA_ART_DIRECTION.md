# Shroudtopia — direção de arte V2

**Para o Claude implementar no app existente · 21/10/2026**

## Leia isto primeiro

Este documento **substitui o guia anterior**. A primeira aplicação mudou a paleta, mas continuou com aparência genérica. O objetivo agora é incorporar as três artes fornecidas ao produto. Uma entrega que apenas ajuste cores, fontes e bordas não atende ao pedido.

O usuário aprovou especialmente `shroudtopia-mist-valley.png`. Essa imagem é a referência principal de atmosfera, iluminação e materiais. A assinatura também foi corrigida: símbolo e nome alinhados pelo centro visual, com espaço entre os dois.

Mantenha as telas, a navegação e os fluxos atuais. Não é uma solicitação para redesenhar o aplicativo ou gerar imagens das telas. É uma integração de arte ao que já existe. Pequenos ajustes locais para acomodar um emblema são permitidos; reconstruir a composição da janela não é necessário.

## 1. Direção escolhida

**Um refúgio entre as névoas.** Ruínas de pedra cobertas de musgo, profundidade azulada, vegetação e um ponto de luz âmbar. A presença do mundo vem de ilustração, material e luz, além da paleta.

Os três assets formam um conjunto: o cenário mostra o lugar; o santuário traz a Chama para perto; o medalhão representa o mundo compartilhado. A marca simples se mantém legível sobre essa arte mais rica.

Não espalhe mais fantasia por iniciativa própria. Esses elementos já dão identidade suficiente. Ícones funcionais, textos e controles continuam claros e convencionais.

## 2. Arquivos e usos obrigatórios

| Arquivo | Uso principal | Escala inicial |
|---|---|---|
| `shroudtopia-mist-valley.png` | Cenário na parte inferior do conteúdo principal, visível na área vazia abaixo dos cartões | Largura da área principal, proporção original |
| `shroudtopia-flame-shrine.png` | Ornamento na área livre da sidebar, entre navegação e lista de pessoas | 138 px de largura; altura automática |
| `shroudtopia-world-medallion.png` | Emblema ao lado do nome e estado do mundo, dentro do cabeçalho atual do cartão | 48 × 48 px; até 56 × 56 px se houver espaço |
| `shroudtopia-logo.svg` | Marca completa no cabeçalho da sidebar, substituindo a versão anterior | 176–184 px de largura; altura automática |
| `shroudtopia-logo.png` | Alternativa raster da mesma assinatura corrigida | Mesma proporção do SVG |
| `shroudtopia-mark.svg` / `.png` | Símbolo sozinho, apenas se houver um uso compacto real | 24–32 px |
| `shroudtopia-theme.css` | Referência de cores e estados, mantida do pacote anterior | Adaptar aos componentes existentes |
| `shroudtopia-art.css` | Referência de integração, camadas, dimensões e espaçamento dos assets V2 | Adaptar ao projeto real |

Os PNGs de santuário e medalhão têm transparência. O cenário é opaco, com a parte superior escura preparada para se integrar ao fundo. Não tratar o preto de um visualizador como parte obrigatória dos recortes transparentes.

## 3. Cenário: deixar a arte aparecer

Use `shroudtopia-mist-valley.png` como uma camada decorativa presa ao fundo da área principal, alinhada ao rodapé e ocupando toda a largura dessa área. A imagem não deve atravessar a sidebar.

**Comece em opacidade 0,90.** A imagem já tem sombras e uma região superior calma. Não colocar por padrão uma camada preta de 70–90% por cima, nem reduzir a arte a 5–15% de opacidade. Isso repetiria o resultado rejeitado.

Preserve a proporção 3:2, começando com largura de 100% e altura automática. Em uma área de aproximadamente 500 px de largura, ela ocupa cerca de 333 px de altura. A parte superior escura fica atrás da região dos cartões; as ruínas aparecem sobretudo no espaço livre inferior. O arco iluminado à direita precisa continuar reconhecível.

Os cartões e linhas de informação ficam acima da imagem, em superfícies opacas ou quase opacas. A legibilidade vem dessas superfícies, não de apagar todo o cenário. Títulos de página que fiquem fora dos cartões devem permanecer na zona escura superior.

Se a emenda superior aparecer, use uma máscara gradual apenas no topo da imagem. Não escurecer a composição inteira. Não esticar para preencher toda a altura da janela. Evitar `cover` sem avaliar o recorte, pois ele pode cortar o arco e a luz aprovados pelo usuário.

O cenário é decorativo e estático: sem responder ao mouse, sem parallax e sem representar progresso real de sincronização.

## 4. Santuário: ocupar o vazio da sidebar

Use `shroudtopia-flame-shrine.png` **uma vez** na região livre entre a navegação e o bloco “NO MUNDO”. Centralize horizontalmente dentro da largura útil da sidebar.

Na janela de referência, começar com 138 px de largura e cerca de 140 px de distância entre a borda inferior da imagem e o rodapé interno da sidebar. São valores de partida; o bloco de pessoas é o limite real. Deixe ao menos 12–16 px livres antes da divisória desse bloco.

Não empurre os itens de navegação nem a lista de pessoas para acomodá-lo. Em janelas menores, reduza primeiro para 110 px; se ainda houver colisão, oculte apenas o ornamento. Usuários e ações têm prioridade.

Preserve a cor original, o musgo, a transparência e a luz da chama. Opacidade inicial entre 0,90 e 1. Não colocar em cartão, círculo, quadrado colorido ou moldura adicional. Não aplicar outro brilho: a própria arte já contém luz.

Ele não é um botão, não recebe foco e não indica conexão, sincronização ou jogo aberto. Não fazer a chama piscar quando alguém entrar no mundo.

## 5. Medalhão: dar identidade ao mundo

Use `shroudtopia-world-medallion.png` à esquerda do bloco que contém o nome do mundo e seu estado. Alinhe o medalhão ao centro vertical desse conjunto, com **12 px de intervalo**. O botão Jogar/Entrar permanece à direita.

O ajuste é local: inserir uma imagem de 48 px e recuar o bloco de texto o necessário. Preservar altura do cabeçalho sempre que couber. Na referência “Mundo do bang”, há espaço para testar essa solução sem mover o histórico nem aumentar o cartão inteiro.

Para nomes longos, respeitar a solução atual de quebra/truncamento e manter o botão acessível. Se necessário, reduzir o medalhão para 40 px. Não encolher o texto ou inventar uma segunda linha de ações para manter uma imagem maior.

O medalhão representa o mundo, não o avatar de uma pessoa. Não substituir todos os avatares por ele. Não recolorir o desenho por estado: “Em dia”, hospedagem, conflito e falha continuam com seus indicadores e textos próprios.

Na tela de conflito, pode permanecer no cabeçalho, mas não deve se repetir em cada alternativa de versão.

## 6. Assinatura V2: centralizada e com respiro

Use os novos arquivos `shroudtopia-logo.svg` ou `shroudtopia-logo.png`. Eles substituem a assinatura do primeiro pacote. A composição continua horizontal.

O nome foi baixado em relação à primeira assinatura V2 para alinhar o corpo principal das letras ao centro visual do ícone. O descendente da letra “p” não deve definir esse alinhamento. Há **24 unidades de intervalo no vetor**, equivalentes a aproximadamente **6,6 px quando a assinatura mede 176 px de largura**. O SVG tem quadro 644 × 118. O PNG correspondente tem 2576 × 472 px e transparência.

Não usar o contorno da caixa de imagem como referência para uma nova montagem manual: o espaçamento e o alinhamento já estão resolvidos dentro do asset. Não acrescentar texto “Shroudtopia” ao lado da assinatura completa.

No componente do cabeçalho, centralizar a assinatura verticalmente na área que já existe. Se esse componente tiver largura útil definida e for desejado centralizar a marca nela, usar alinhamento ao centro sem mudar a largura da sidebar. Manter pelo menos 8 px de folga externa em relação à divisória e às bordas úteis.

O emblema simples foi mantido porque precisa funcionar em tamanho pequeno e já compartilha a forma de arco/chama do santuário. O nome permanece em marfim, sem textura, relevo ou brilho. **A arte mais rica fica nos outros assets; a assinatura não precisa virar uma miniatura de pedra ilegível.**

## 7. Materiais, texto e controles

Manter a base de ardósia escura, texto em marfim, ações quentes e azul claro para conexão/foco. `shroudtopia-theme.css` contém a paleta anterior, que pode continuar. O salto visual desta versão está na integração das artes.

Não transformar todos os cartões em lajes fotografadas ou em molduras grossas. Os materiais pintados das três imagens já trazem essa informação. Os controles permanecem de acabamento simples, com bordas discretas e estado visível.

Texto de corpo e histórico em uma fonte de UI legível. Serifa pode continuar em títulos curtos, se já tiver sido adotada, mas não precisa se espalhar para horários, caminhos, opções de mods e mensagens. Evitar mais mudanças de fonte apenas para tentar compensar ausência de arte.

O status “Enshrouded fechado” é neutro. Sucesso, atenção e falha não devem depender exclusivamente de cor. Origem de arquivo (“neste PC”, “no servidor”) não significa versão escolhida.

Não aplicar opacidade ao contêiner que engloba arte e controles: isso também apagaria os textos. Ajustar somente a camada decorativa.

## 8. Como aplicar às telas existentes

| Tela/estado | Aplicação |
|---|---|
| Mundo em dia | Cenário no rodapé da área principal; santuário na sidebar; medalhão no cabeçalho do mundo. Manter histórico e botão no mesmo fluxo. |
| Amigo hospedando | Mesmos assets e posições. A diferença de estado permanece nos textos, indicadores e ação Steam, não na paisagem. |
| Versões separadas | Arte atrás das superfícies e santuário na sidebar. Atenção e alternativas seguem prioritárias; não favorecer uma versão com imagens diferentes. |
| Mods | Cenário como fundo inferior; santuário na sidebar. Grupos e switches mantêm seus tamanhos e funcionamento. Não espalhar medalhões em cada mod. |
| Atualização | Cenário no fundo; santuário na sidebar. Notas, versão, progresso e configurações ficam em superfícies legíveis acima da arte. |
| Entrada/compartilhamento | Cenário no rodapé do conteúdo. Como não há a sidebar habitual nessa composição, não forçar a inclusão do santuário. Os dois cartões de escolha continuam equivalentes. |

Quando houver mais conteúdo e menos área vazia, é normal que parte da paisagem fique coberta. Não afastar os cartões para exibir mais ilustração. Na tela de Mundo enviada pelo usuário, porém, a região inferior está livre: ali o cenário deve aparecer com clareza.

## 9. Integração técnica sem inventar uma arquitetura

Inspecione o repositório e siga suas instruções locais. As referências são capturas; este pacote não pressupõe React, Electron, Tauri, WPF ou outra tecnologia.

Copiar os arquivos para a raiz é a forma de entrega, não garantia de que o executável empacotado irá encontrá-los. Integre as imagens ao mecanismo de recursos real, confirme caminhos e verifique funcionamento offline.

`shroudtopia-art.css` é uma referência para interfaces que usam CSS. Ele não tem URLs prontas do projeto: associe os assets através dos imports ou caminhos resolvidos pelo empacotador. Se o projeto for nativo, traduza proporções, camadas e alinhamento para seus componentes.

Camadas: fundo sólido → cenário decorativo → conteúdo funcional em superfícies legíveis. Na sidebar: fundo → santuário decorativo → navegação/pessoas. Não colocar uma imagem transparente por cima dos cliques. Imagens decorativas devem ser ignoradas pela acessibilidade; a assinatura completa recebe o nome acessível “Shroudtopia”.

Não criar carregamento remoto, dependência de fontes online ou biblioteca de animação. Não modificar lógica de hospedagem, sincronização, mods ou atualização.

## 10. Revisão que comprova o resultado

1. Na mesma tela enviada pelo usuário, é possível reconhecer o vale, as ruínas e o arco iluminado na região inferior sem ampliar a imagem?
2. O santuário está visível, centralizado e separado da lista de pessoas?
3. O medalhão cabe no cabeçalho sem esmagar o nome e o botão?
4. Símbolo e nome da assinatura estão centralizados entre si, com intervalo evidente? A assinatura está centralizada verticalmente no cabeçalho?
5. Texto, foco, switches e alertas continuam legíveis? Controles não dependem da textura atrás deles para serem reconhecidos?
6. Em janela menor e escalas de sistema de 125% e 150%, a arte cede espaço antes das ações e informações?
7. Os recursos aparecem na versão empacotada e offline?

Compare os mesmos estados antes/depois e faça uma verificação visual no aplicativo real. Não considerar concluído apenas porque o CSS importou. O resultado deve mostrar as artes, além das cores.

Para texto normal, manter contraste de pelo menos 4,5:1; para informação visual necessária de controles/estados, 3:1. Fundos de cartões são o meio principal de preservar isso. Referências: [WCAG — texto](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) e [contraste não textual](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).

## 11. Referências e procedência

A pesquisa original consultou o [site oficial de Enshrouded](https://enshrouded.com/en-US), a [imagem oficial de exploração](https://enshrouded.com/uploads/EN_STEAM_EXPLORATION_214276e62e.jpg), a [imagem oficial de crafting com HUD](https://enshrouded.com/uploads/EN_STEAM_CRAFTING_06c0afe612.jpg) e o [press kit da Keen Games](https://download.enshrouded.com/Press%20Kit/). Essas referências orientaram a relação entre ruínas, abrigo, névoa e luz; as medidas e escolhas de aplicação deste documento são propostas para Shroudtopia.

As três ilustrações deste pacote foram geradas com a ferramenta nativa de imagens, uma por chamada. O cenário foi usado como referência visual das outras duas para manter o conjunto coerente. Não são screenshots, elementos extraídos do jogo nem protótipos de tela.

A assinatura foi ajustada em vetor a partir da versão anterior; os PNGs da marca são exportações dos vetores. Nenhum arquivo de fonte é necessário para exibir a assinatura. Os prompts exatos das três ilustrações estão em `PROMPTS_GERACAO.md`.

O pacote foi verificado como conjunto de assets. A integração no aplicativo real continua sendo trabalho do Claude com acesso ao repositório.
