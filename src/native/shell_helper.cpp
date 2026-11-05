// Shell extensions remain in this disposable STA process, never the GPUI process.
#define NOMINMAX
#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <cstdio>
#include <iostream>
#include <string>
#include <vector>
#include <array>
#include <stdexcept>
IContextMenu2*menu2=nullptr;IContextMenu3*menu3=nullptr;
LRESULT CALLBACK proc(HWND h,UINT m,WPARAM w,LPARAM l){LRESULT result=0;if(menu3&&(m==WM_INITMENUPOPUP||m==WM_UNINITMENUPOPUP||m==WM_DRAWITEM||m==WM_MEASUREITEM||m==WM_MENUCHAR)&&SUCCEEDED(menu3->HandleMenuMsg2(m,w,l,&result)))return result;if(menu2&&(m==WM_INITMENUPOPUP||m==WM_DRAWITEM||m==WM_MEASUREITEM||m==WM_MENUCHAR)&&SUCCEEDED(menu2->HandleMenuMsg(m,w,l)))return 0;return DefWindowProcW(h,m,w,l);}
void require(bool value){if(!value)throw std::runtime_error("shell");}
void exact(HANDLE h,void*data,DWORD n){DWORD count=0;require(ReadFile(h,data,n,&count,nullptr)&&count==n);}
std::wstring text(HANDLE h){uint32_t n=0;exact(h,&n,4);require(n>0&&n<32760);std::wstring s(n,0);exact(h,s.data(),n*2);require(s.find(L'\0')==std::wstring::npos);return s;}
std::array<unsigned char,33> identity(const std::wstring&p){HANDLE h=CreateFileW(p.c_str(),FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr);require(h!=INVALID_HANDLE_VALUE);BY_HANDLE_FILE_INFORMATION b{};FILE_ID_INFO v{};std::array<unsigned char,33>r{};bool ok=GetFileInformationByHandle(h,&b);if(GetFileInformationByHandleEx(h,FileIdInfo,&v,sizeof(v))){r[0]=1;memcpy(r.data()+1,&v.VolumeSerialNumber,8);memcpy(r.data()+9,v.FileId.Identifier,16);}else{uint64_t volume=b.dwVolumeSerialNumber,id=((uint64_t)b.nFileIndexHigh<<32)|b.nFileIndexLow;memcpy(r.data()+1,&volume,8);memcpy(r.data()+9,&id,8);}CloseHandle(h);require(ok);uint64_t created=((uint64_t)b.ftCreationTime.dwHighDateTime<<32)|b.ftCreationTime.dwLowDateTime;memcpy(r.data()+25,&created,8);return r;}
// Check the filesystem object represented by the parsed PIDL, not just its spelling.
PIDLIST_ABSOLUTE parse(const std::wstring& path,const std::array<unsigned char,33>& expected){
 PIDLIST_ABSOLUTE pidl=nullptr;require(SUCCEEDED(SHParseDisplayName(path.c_str(),nullptr,&pidl,0,nullptr)));
 IShellItem* item=nullptr;PWSTR resolved=nullptr;
 HRESULT hr=SHCreateItemFromIDList(pidl,IID_PPV_ARGS(&item));
 if(SUCCEEDED(hr))hr=item->GetDisplayName(SIGDN_FILESYSPATH,&resolved);
 bool same=false;try{same=SUCCEEDED(hr)&&resolved&&identity(resolved)==expected;}catch(...){}
 if(resolved)CoTaskMemFree(resolved);if(item)item->Release();
 if(!same){CoTaskMemFree(pidl);require(false);}return pidl;
}
int wmain(int argc,wchar_t**argv){try{
  require(argc==6);SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOGPFAULTERRORBOX);SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
  require(SUCCEEDED(CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED)));
  HWND parent=(HWND)(uintptr_t)_wcstoui64(argv[2],nullptr,10);DWORD pid=0;GetWindowThreadProcessId(parent,&pid);require(IsWindow(parent)&&pid==wcstoul(argv[3],nullptr,10));
  HANDLE file=CreateFileW(argv[1],GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);require(file!=INVALID_HANDLE_VALUE);
  uint32_t count=0;exact(file,&count,4);require(count<=2048);auto folderPath=text(file);std::array<unsigned char,33>folderId;exact(file,folderId.data(),33);require(identity(folderPath)==folderId);
  std::vector<std::wstring>paths;std::vector<std::array<unsigned char,33>>ids;size_t units=folderPath.size();
  for(uint32_t i=0;i<count;i++){paths.push_back(text(file));units+=paths.back().size();require(units<=256*1024);std::array<unsigned char,33>id;exact(file,id.data(),33);require(identity(paths.back())==id);ids.push_back(id);}CloseHandle(file);
  WNDCLASSW wc{};wc.lpfnWndProc=proc;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"BackleafShellHost";RegisterClassW(&wc);HWND owner=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_LAYERED,wc.lpszClassName,L"",WS_POPUP,0,0,1,1,parent,nullptr,wc.hInstance,nullptr);require(owner!=nullptr);
  IShellFolder*desktop=nullptr;IShellFolder*folder=nullptr;require(SUCCEEDED(SHGetDesktopFolder(&desktop)));PIDLIST_ABSOLUTE root=parse(folderPath,folderId);require(SUCCEEDED(desktop->BindToObject(root,nullptr,IID_PPV_ARGS(&folder))));
  IContextMenu*context=nullptr;std::vector<PIDLIST_ABSOLUTE>absolute;std::vector<PCUITEMID_CHILD>children;
  for(size_t i=0;i<paths.size();i++){PIDLIST_ABSOLUTE item=parse(paths[i],ids[i]);require(ILIsParent(root,item,TRUE));absolute.push_back(item);children.push_back(ILFindLastID(item));}
  if(count){require(SUCCEEDED(folder->GetUIObjectOf(owner,count,children.data(),IID_IContextMenu,nullptr,(void**)&context)));}
  else{require(SUCCEEDED(folder->CreateViewObject(owner,IID_IContextMenu,(void**)&context)));}
  context->QueryInterface(IID_PPV_ARGS(&menu3));if(!menu3)context->QueryInterface(IID_PPV_ARGS(&menu2));
  HMENU menu=CreatePopupMenu();require(menu!=nullptr);require(SUCCEEDED(context->QueryContextMenu(menu,0,1,0x7fff,CMF_NORMAL|CMF_EXPLORE)));
  std::cout<<"shown\n"<<std::flush;SetLayeredWindowAttributes(owner,0,0,LWA_ALPHA);ShowWindow(owner,SW_SHOWNOACTIVATE);SetForegroundWindow(owner);
  int x=_wtoi(argv[4]),y=_wtoi(argv[5]);UINT command=TrackPopupMenuEx(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,x,y,owner,nullptr);
  if(command){require(identity(folderPath)==folderId);for(size_t i=0;i<paths.size();i++)require(identity(paths[i])==ids[i]);SetWindowLongPtrW(owner,GWLP_HWNDPARENT,0);std::cout<<"invoking\n"<<std::flush;CMINVOKECOMMANDINFOEX info{};info.cbSize=sizeof(info);info.fMask=CMIC_MASK_UNICODE|CMIC_MASK_PTINVOKE;info.hwnd=owner;info.lpVerb=MAKEINTRESOURCEA(command-1);info.lpVerbW=MAKEINTRESOURCEW(command-1);info.nShow=SW_SHOWNORMAL;info.ptInvoke={x,y};require(SUCCEEDED(context->InvokeCommand((LPCMINVOKECOMMANDINFO)&info)));}
  if(!command&&IsWindow(parent))SetForegroundWindow(parent);
  DestroyMenu(menu);if(menu3)menu3->Release();if(menu2)menu2->Release();context->Release();folder->Release();desktop->Release();for(auto p:absolute)CoTaskMemFree(p);CoTaskMemFree(root);DestroyWindow(owner);CoUninitialize();std::cout<<"closed\n"<<std::flush;return 0;
}catch(...){std::cout<<"error\n"<<std::flush;return 1;}}
